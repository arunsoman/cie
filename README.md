# cie — the only code graph that knows which tasks and tests actually implement your code.

[![CI](https://github.com/kannamma-labs/cie/actions/workflows/ci.yml/badge.svg)](https://github.com/kannamma-labs/cie/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/kannamma-labs/cie?include_prereleases&label=release)](https://github.com/kannamma-labs/cie/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Python 3.10+](https://img.shields.io/badge/python-3.10+-blue.svg)](https://www.python.org/downloads/)
[![MCP](https://img.shields.io/badge/MCP-server-7c3aed.svg)](https://modelcontextprotocol.io)
[![tree-sitter](https://img.shields.io/badge/extraction-tree--sitter-4A9043.svg)](https://tree-sitter.github.io/tree-sitter/)
[![Neo4j](https://img.shields.io/badge/backend-Neo4j%20%7C%20SQLite-008CC8.svg)](https://neo4j.com)
[![Tests](https://img.shields.io/badge/tests-213%20passing-success.svg)](tests/)
[![Keep a Changelog](https://img.shields.io/badge/changelog-Keep%20a%20Changelog-06b6d4.svg)](CHANGELOG.md)

[![GitHub issues](https://img.shields.io/github/issues/kannamma-labs/cie?logo=github&label=issues)](https://github.com/kannamma-labs/cie/issues)
[![PRs](https://img.shields.io/github/issues-pr/kannamma-labs/cie?logo=github&label=PRs)](https://github.com/kannamma-labs/cie/pulls)
[![Contributors](https://img.shields.io/github/contributors/kannamma-labs/cie?logo=github)](https://github.com/kannamma-labs/cie/graphs/contributors)
[![Stars](https://img.shields.io/github/stars/kannamma-labs/cie?style=social&logo=github)](https://github.com/kannamma-labs/cie/stargazers)
[![Last commit](https://img.shields.io/github/last-commit/kannamma-labs/cie?logo=github)](https://github.com/kannamma-labs/cie/commits/main)
[![Commit activity](https://img.shields.io/github/commit-activity/y/kannamma-labs/cie?logo=github)](https://github.com/kannamma-labs/cie/commits/main)
[![Code size](https://img.shields.io/github/languages/code-size/kannamma-labs/cie?logo=github)](https://github.com/kannamma-labs/cie)
[![Repo size](https://img.shields.io/github/repo-size/kannamma-labs/cie?logo=github)](https://github.com/kannamma-labs/cie)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)](#install)
[![Status](https://img.shields.io/badge/status-alpha-orange.svg)](https://github.com/kannamma-labs/cie/releases)

*Code Insight Engine.* No other surveyed code-graph tool can answer
"which files implement this task, and are they tested?" as one query —
they're all pure retrieval. cie can, because task/QA traceability lives
in the same graph as the code. It also extends to languages with no LSP
and no tree-sitter grammar (proven on Nirdosha, a from-scratch language,
via nothing but the compiler's own AST dump).

![A real `cie-mcp` server answering "who really calls close()?" against psf/requests over the actual Model Context Protocol — `close()` is defined 4 times in that codebase, grep finds 6 raw matches with no way to tell which class each belongs to, callers() resolves 3 real ones through the actual call graph](./demo.svg)

*Every line above is a real command against a real clone of
[`psf/requests`](https://github.com/psf/requests) (52k+ stars, not this
project's own code) — `cie index .`, then a real MCP stdio client
calling `callers("close")` on a running `cie-mcp --embedded` server.
Reproduce it yourself: [`scripts/record_demo.sh`](scripts/record_demo.sh).
Full methodology, including where this exact query under-resolves (3 of
6 real call sites, a real gap not hidden here) is in
[`docs/benchmarks-requests.md`](docs/benchmarks-requests.md).*

**One real number, measured against a real 36-file codebase** (full
methodology in [`docs/benchmarks.md`](docs/benchmarks.md), including a
case where it didn't help): resolving every real caller of an ambiguous
function name took **1** cie tool call (`callers()`, correct-by-
construction on every result it returns) vs. **3** for grep-only (1 grep
+ 2 reads to disambiguate, still not guaranteed correct). Not every task
favors a graph — the same doc reports a tie and a real loss, honestly,
not just the wins — and re-run on a second, independent public repo
([`psf/requests`](https://github.com/psf/requests), not this project's
own code) in [`docs/benchmarks-requests.md`](docs/benchmarks-requests.md),
the pattern holds on a real win (a 1,184-line file skeletonizes to 43%
of its raw size) and surfaces a real miss too (the same ambiguous-caller
query resolved only 3 of 6 real call sites on that repo) — published
because it's true, not adjusted to look better. A **third** dataset —
[urllib3](https://github.com/urllib3) — adds a reproducible harness
([`scripts/benchmark.sh`](scripts/benchmark.sh)): every number in
[`docs/benchmarks-urllib3.md`](docs/benchmarks-urllib3.md) regenerates
from the script, including the published recall miss (28 of 40 `close`
call sites unresolved, in the response, not hidden).

**A second hook, also measured, not asserted:** cie ships ~135
LLM-callable tools — not a generic "run arbitrary code" surface the
model has to improvise a workaround from, but specific ones (`callers`,
`file_skeleton`, `traceability_orphans`...) that let it express intent
directly. The obvious worry is that more tools means more chances to
pick the wrong one — [tested it](docs/tool-selection-accuracy.md)
instead of assuming: a fresh agent, given cie's real tool list plus 14
tasks hand-picked to be confusable (5 different "coverage"-named tools
alone),
picked the exactly correct tool **14/14** — against the full read-only
surface measured that run (81 tools at the 2026-08-30 snapshot; the
surface has since grown — today 135 ToolService tools / 85 read-only
under the `inspector` policy) and the same 14/14 it got against a 14-tool
subset. One run, real caveats in the linked doc — but the
"more tools, more room to mess up" worry didn't hold up when actually
checked.

**Tool-count labels — one convention, introspection-derived** (no prose
estimate is ever cited without its label):

- **135 ToolService tools** — every public `ToolService` method minus
  `describe`; what `cie-mcp` serves under `--policy full` and what
  `POST /tools/{tool}` accepts.
- **85 read-only tools** — the `--policy inspector` (default HTTP) view;
  135 minus the 50 `WRITE_TOOLS` members (pinned in `tests/
  test_tool_surface_invariants.py`).
- Historical snapshots (81/121/etc.) are dated record — CHANGELOG keeps
  them labeled; live cuts re-measure from introspection, never edit
  history.

Try it in two commands, no server, no signup — index a project into a
local SQLite file and serve it to Claude Code, Cursor, or any MCP client,
task/QA traceability included. Or one command to do all of it:

```bash
cie init /path/to/your/project
```

Detects installed MCP clients (Claude Code via project `.mcp.json`,
Cursor via `~/.cursor/mcp.json`), registers the stdio server
(idempotent, existing entries untouched), and writes managed context
blocks into `AGENTS.md`/`CLAUDE.md`. Default policy is **readonly** —
the client gets the read tools until you pass `--policy full` (the
opt-in, not the default). Codex is detected and the exact TOML snippet
printed (never auto-edited). Point it at Neo4j instead for a real
team/multi-project setup (see Quickstart below for what's in each mode).

See [`docs/competitive-landscape.md`](docs/competitive-landscape.md) for
the full comparison against CodeGraph, CodeGraphContext, Serena, and
others, including where cie is honestly behind.

## Quickstart (zero-config, no Neo4j)

```bash
pip install "cie[mcp]"
cie index /path/to/your/project
cie-mcp /path/to/your/project --embedded
```

That's an MCP server over stdio — add it to Claude Code / Cursor / Codex /
any MCP client the way you'd add any other local MCP server, and it can
call `search_symbol`, `callers`, `callees`, `file_skeleton`,
`path_between`, and everything else in `cie.tools.ToolService` against
your project's real call graph, indexed locally in `.cie/graph.db`.

`--policy inspector` (read-only) is available if you want the connecting
client to only ever see read tools — see
[`cie/tool_policy.py`](cie/tool_policy.py). Task/QA tracking works here
too, backed by a second local SQLite file (`.cie/tasks.db`, via
`cie.embedded_task_repository.EmbeddedTaskRepository`) — pass
`--no-task-tracking` to `cie-mcp` if you'd rather skip creating it.

The hierarchy store is `--no-hierarchy`-optional the same way
(`.cie/hierarchy.db` by default).

### Share it: one static HTML file

```bash
cie export-html /path/to/project --out snapshot.html
```

One self-contained file (task→file→test chains, orphan symbols, the
task list, indexed files) — no server, no auth surface, no network
calls; open it via `file://` and share it. The zero-external-reference
contract is asserted by tests and re-proven by
[`scripts/record_export_html.sh`](scripts/record_export_html.sh)
(screenshots in `docs/images/`). This is the *safe slice* of a viewer:
a snapshot, not an app.

### Serving over HTTP instead of stdio

```bash
cie-mcp /path/to/project --embedded --policy inspector \
  --transport streamable-http --host 127.0.0.1 --port 8000
```

Browser/wire MCP clients (e.g. MCP Inspector in browser mode) connect
to `http://127.0.0.1:8000/mcp`. The SAME `ToolPolicy` filters
registration on every transport — server-side, verified per transport
(`tool-test-lab/dogfood_mcp_http.py`: HTTP `tools/list` == exactly the
policy's predicted set; a write call comes back refused). Loopback bind
by default; widening `--host` makes it a network service — see
[`docs/security.md`](docs/security.md) for the `run`-tool boundary.

**Query it from the CLI too, same file, still no Neo4j** — the documented
query commands answer from the same `.cie/graph.db` the index wrote
(auto-selection: embedded when a local graph.db exists, `--backend`/
`CIE_BACKEND` to override; roadmap R2):

```bash
cie files                     # what's indexed
cie search-symbol close       # definitions by name
cie callers close             # blast radius (resolved call graph)
cie callees close
cie skeleton src/api.py
cie path alpha helper
cie tasks:pending             # the task/QA layer, from .cie/tasks.db
```

Every command honors `--json` (group-level, before the subcommand) for
the machine-driven, and explicit `--backend embedded/neo4j` + `--db`
override the selection rule hierarchy. `cie load`/`cie watch`/`cie
bootstrap` remain the multi-project Neo4j ingest paths (their embedded
counterpart is re-running `cie index`).

See "What it is — three layers" below for the full breakdown (structural
extraction, ~135 tools, the task/QA layer).

## Install

```bash
pip install cie             # core: graph, tools, task/hierarchy layer over Neo4j
pip install "cie[mcp]"      # + the MCP server (cie-mcp) — what most people want
pip install "cie[http]"     # + the HTTP tool-mount / mock server (cie/routes.py)
```

Core dependencies (`pyproject.toml`): Neo4j driver, Pydantic v2,
tree-sitter (+ Python/JS/TS/Java/Go/Rust/C/C++/C# grammars), watchdog, Click,
Rich.
Requires Python ≥ 3.10. Only `routes.py` / `mock_server.py` pull in
FastAPI/uvicorn (the `[http]` extra); only `mcp_server.py` pulls in the
MCP SDK (the `[mcp]` extra). The query engine, extraction, task/hierarchy
repos, and `ToolService` itself have **no HTTP dependency at all**.

---

## What it is — three layers

- **A generic code graph.** Structural extraction (symbols, call graph,
  imports, inheritance, test links) via pluggable `LanguageAdapter`s —
  ships with tree-sitter support for **Python / JavaScript / TypeScript
  / Java / Go / Rust** out of the box (Go/Rust: function+method
  extraction, signatures, and receiver/impl-method call resolution;
  import-edge extraction and docstrings are a documented gap for these
  two — see `cie/extract.py`'s module docstring); add your own adapter
  for any other language
  (wrapping a compiler's own AST dump, an LSP server, or a tree-sitter
  grammar) via `cie.lang_adapter.register_adapter` or the
  `cie.language_adapters` entry-point group, **no code change to this
  package required**.
- **~135 LLM-callable tools** (`cie.tools.ToolService`, exposed 1:1 as
  MCP tools and `POST /tools/{tool}` endpoints) — symbol search,
  call-graph traversal, clone/community/drift detection, quality reports,
  test-intelligence, traceability, confidence scoring, decomposition,
  APM, a jailed virtual filesystem
  (`view_file`/`write_file`/`edit_file`/`delete_file`/`write_files_atomic`),
  and a repair transaction layer (`propose_patch`/`apply_patch`/
  `verify_patch` over immutable PatchPlan nodes — see
  [`cie/patch.py`](cie/patch.py) and the changelog),
  all self-describing (`ToolService.describe()`), exposable as typed
  JSON-Schema tool definitions (`cie.tool_schema`) with per-agent-type
  authorization (`cie.tool_policy`), servable over the real Model
  Context Protocol (`cie.mcp_server`, `cie-mcp`).
- **A task / PRD-hierarchy layer** (`cie.task_repository`,
  `cie.hierarchy`) for tracking atomic dev/QA tasks and (optionally) a
  project's PRD decomposition tree. Task/QA CRUD and traceability
  (`cie.task_repository.TaskRepository` — push/list/status, dependency
  traversal, coverage/cycle/API-contract validation) works zero-config
  too, via `cie.embedded_task_repository.EmbeddedTaskRepository`
  (SQLite, `.cie/tasks.db`; pass `task_tracking=False` to
  `build_tool_service_embedded`, or `--no-task-tracking` to `cie-mcp`,
  for `cie.embedded_repository.NullTaskRepository`'s fail-fast behavior
  instead). The separate PRD-decomposition tree (`cie.hierarchy`) works
  here too — the SQLite PRD-hierarchy store
  (`cie.embedded_hierarchy_repository.SQLiteHierarchyRepository`, R14;
  default `.cie/hierarchy.db`, or `--no-hierarchy` /
  `hierarchy_tracking=False` to opt out) implements the same
  `HierarchyRepository` protocol the Neo4j backend does.
- **Honest degradation, machine-readable** (post-R5): tools whose logic
  is pure run standalone — the 2026-08-30 lazy-`core.llm` refactor
  un-503'd 13 of the 18 previously unavailable tools — and the 5 that
  genuinely need a host-only backend (`qa`, `contracts_run`,
  `state_machine_run`, `community_summarize_run`: the host's LLM layer;
  `decompose_page`: a decompose plugin) return `kind="unavailable"`
  **with a stable `error.reason` slug** (e.g.
  `OPTIONAL_BACKEND_MISSING:core`), pinned by
  `tests/test_unavailable_reasons.py`, plus a machine-checked gate that
  the unavailable bucket can't quietly regrow.

## Capabilities (grounded in the code)

The `cie/` package is ~28k lines across ~60 modules. The capability
surface maps cleanly onto the spec sections the code itself documents in
its module docstrings. Nothing below is aspirational — each bullet is a
real module and (where noted) a real tool on `ToolService` / the CLI /
the HTTP routes.

### Two-pass code-graph extraction & loading (`extract.py`, `callgraph.py`, `testlink.py`)
- Pass 1 (`extract.py`): tree-sitter parse of every supported file into
  file/class/function/method `Node`s with `signature`, `line_start`/
  `line_end`, `docstring`, plus the raw inputs for pass 2 — `imports`
  and `call_sites`. Pure: no DB/FS side effects.
- Pass 2 (`callgraph.py`): resolves call sites into confidence-tagged
  `calls` edges — EXTRACTED (same-file def or import-map resolved),
  INFERRED (receiver-type heuristic), or AMBIGUOUS (exactly one same-named
  symbol project-wide). Also resolves `inheritance`/`extends` edges and
  synthesizes `external::` stub nodes for unresolved base classes.
- `testlink.py`: a third pass that emits `TESTS` edges from test symbols
  to the implementation symbols they test, via three heuristics — naming
  convention (`test_foo` → `foo`), confidence upgrade when a naming match
  is backed by a real `calls` edge, and `@patch(...)`/`@mock.patch(...)`
  decorator resolution.
- Loaders: `cie load <dirs> --project <name>` (Neo4j, full replace of one
  project's nodes) and `cie index <path>` (embedded SQLite, zero-config).
  `reindex` / `reindex_file` for incremental single-file refresh after a
  patch; `watch` for file-system-driven auto-reindex (watchdog).

### Core data model (`models.py`, `repository.py`, `neo4j_repository.py`, `in_memory_repository.py`, `embedded_repository.py`)
- `NodeKind` covers the structural kinds (FILE/CLASS/FUNC/METHOD/SYMBOL)
  **and** every analysis-result kind — CloneCluster, AntiPattern,
  DriftFinding, MetricSnapshot, CommunitySummary, Type, Package, Document,
  Contract, TestSkeleton, StateMachine, State, Transition, AgentVerdict,
  ConfidenceReport, JustificationTrace, InvariantViolation,
  SemanticDiffFinding, RuntimeErrorTrace, Page, ImpliedPage,
  InteractiveElement, DerivedTaskHint, TestExecution, MockEndpoint,
  MockCall, ContractViolation, ApmMetric, PerformanceBaseline,
  PerformanceRegression, CoverageGap. Analysis nodes are never produced
  by `extract.py` — only by on-demand passes, written via
  `replace_analysis_nodes`.
- `Edge` confidence: EXTRACTED / INFERRED / AMBIGUOUS, stamped with
  IN-08 provenance (`extracted_at`, `extractor_version`, `source_ref`).
- Three `Repository` backends behind one Protocol: `Neo4jRepository`
  (Cypher, per-project namespacing, vector index, query/write/schema
  timeouts), `InMemoryRepository` (the reference test double both backends
  are verified against), and `EmbeddedRepository` (SQLite, two tables,
  full graph re-persisted per call — simple, single-project, local-first).
- `QueryEngine` (`query.py`): thin, backend-agnostic orchestration —
  search, traversal, neighbors, community, god nodes, stats, shortest
  path, signatures, methods-of-class, file listing, feature discovery,
  semantic search (requires embeddings written at load time).

### Storage backends & config (`config.py`, `factory.py`)
- `Neo4jConfig.from_env()` — reads `NEO4J_*` (or legacy `CIE_NEO4J_*`
  override) plus per-operation timeouts
  (`CIE_NEO4J_QUERY_TIMEOUT_S`, `..._WRITE_TIMEOUT_S`, `..._SCHEMA_TIMEOUT_S`).
  Driver-level bounds alone don't stop a lock-wait hang; `cie.timeouts`
  enforces independent wall-clock budgets around each query round trip.
- `CieConfig` — one explicit bootstrap object for an external caller
  (project root, project name, Neo4j config, allowed root, file-size
  ceiling, language adapters). No "disable the jail" toggle — the file
  tools jail unconditionally (`cie.tools.view._jail`).
- `factory.py` builds `ToolService` three ways: `build_tool_service`
  (Neo4j, per-project cached engines/task-repos sharing one driver),
  `build_tool_service_from_config` (one-call, no env vars), and
  `build_tool_service_embedded` (SQLite graph +
  `EmbeddedTaskRepository` by default, `NullTaskRepository` opt-in via
  `task_tracking=False`).

### Tool surface — `ToolService` (`cie/tools/__init__.py`, ~135 methods)
Every method returns the standard SPEC §0 envelope (`ok`/`tool`/`results`/
`truncated`/`total`/`hint`/`elapsed_ms`, `cie.envelope`); errors carry a
mandatory `hint`. Grouped by capability (all also exposed over MCP and
`POST /tools/{tool}`):

**Core graph navigation** — `search_symbol`, `resolve_import`,
`semantic_search`, `callers`, `callees`, `file_skeleton`, `path_between`,
`failing_context`, `affected_by`, `class_hierarchy`, `test_map`,
`actual_callers`, `dead_code_confirm`, `hybrid_search` (lexical + dense
vector + graph-centrality, with per-component scores), `entity_context`,
`view_file` (windowed, line-numbered, joined with the symbol index).

**GraphRAG Q&A** — `qa` (`cie.graphrag`): a real pipeline —
`query_plan.classify` picks a retrieval strategy, `hybrid_search`
retrieves, `rerank` reorders by an LLM relevance judgment,
`entity_context` expands the neighborhood, and a final LLM call answers
with **citations assembled separately from the graph** (the LLM never
emits citations itself).

**Section 13 — Code Intelligence** (on-demand analysis passes written as
analysis nodes):
- **Clone detection** (`clone_detect.py`, CI-01..05): three fused signals
  — token-Jaccard (copy-paste), AST-shape Jaccard (renamed clones),
  embedding cosine (semantic clones) → `CloneCluster` nodes. Tools:
  `clone_detect_run`, `clone_clusters`, `clone_find`.
- **Performance analysis** (`perf_analyze.py`, CI-06..08): Big-O
  estimation (loop nesting + recursion) written onto FUNC/METHOD nodes,
  plus anti-pattern detection (N+1 queries, nested loops, sync I/O in a
  loop, unbounded growth). Tools: `performance_analyze_run`,
  `performance_profile`, `antipattern_scan`.
- **Drift detection** (`drift_detect.py`, CI-10..12): requirement gaps
  (task file_path vs indexed FILE nodes), API contract drift (reuses
  `api_routes` extraction), architectural drift. Tools:
  `drift_detect_run`, `drift_report`, `architecture_check`.
- **Metrics** (`metrics.py`, CI-19..21): rolls clone/drift/tech-debt
  into append-only `MetricSnapshot`s (trend answerable from history).
  Tools: `metrics`, `tech_debt_report`, `metric_trend`.
- **Communities** (`community_detect.py`, RQ-04/AI-03): label-propagation
  detection (the real write-path behind `Node.community` — previously
  read-only with nothing populating it) + LLM-thematic `CommunitySummary`
  nodes carrying embeddings. Tools: `community_detect_run`,
  `community_summarize_run`, `community_search`.
- **Quality governance**: `accuracy_check`, `freshness_report`,
  `comprehensiveness_report`, `salience_report`.

**Section 0 — Population & Real-Time Sync** (`sync.py`): a two-graph model
(speculative vs canonical), a 4-stage `GateRunner` quality gate, tiered
confidence, symbol-level AST delta + move detection, soft-delete-on-
revert, idempotent commit-linked batch population, sync-event
classification. Tools: `sync_quality_gate`, `sync_promote`, `sync_revert`,
`sync_ast_delta`, `sync_evict_speculative`, `sync_load_commit`,
`configure_layer_rules`, `get_layer_rules`, `install_git_hook`.

**Section 1 — Core Data Model extensions** (`data_model.py`): `export_rdf`,
`related_edges`, `validate_property_constraints`, type-flow resolution
(`type_flow_run`/`type_flow`), dependency-graph (`dependency_graph_run`/
`dependency_graph`), documentation graph from markdown (`doc_graph_run`/
`doc_search`).

**Section 14 — Confidence Framework** (spec-vs-code assurance):
- **Contracts** (`contracts.py`, CF-01..03): `python_assert`-form
  contracts, best-effort binding by name to PRD scope, parameter-name
  domain-type validation, `inject_assertions`/`strip_assertions`.
  Tools: `contracts_run`, `contracts`, `validate_types`,
  `inject_assertions`, `strip_assertions`.
- **Test synthesis** (`test_synthesis.py`, CF-04/05): template-generated
  skeletons across six test types, bound to code via the same `TESTS`
  edges DM-14 uses. Tools: `test_skeletons_run`, `test_skeletons`,
  `test_coverage`.
- **State machines** (`state_machine.py`, CF-06/07): FSM extraction,
  dead/unreachable-state detection (real graph algorithms), structural
  code-vs-FSM check. Tools: `state_machine_run`, `state_machine`,
  `fsm_validate`.
- **Traceability** (`traceability.py`, CF-08/09): graph-traversal
  coverage/orphans/chain on the code side **and** the PRD-hierarchy side.
  Tools: `traceability_coverage`, `traceability_orphans`,
  `traceability_chain`, `prd_traceability_coverage`,
  `prd_traceability_orphans`, `prd_traceability_chain`.
- **Semantic diff** (`semantic_diff.py`, CF-10/11): pattern-matching
  spec-vs-code check (deliberately conservative, high false-negative by
  design). Tool: `semantic_diff`.
- **Multi-agent consensus** (`consensus.py`, CF-12/14): verdict storage +
  query (a durable exactly-once bus is explicitly *not* built here).
  Tools: `record_verdict`, `agent_verdicts`.
- **Confidence scoring** (`confidence.py`, CF-15/16): pure composition
  over contract/test/consensus signals; generation/runtime layers reported
  as `None`. Tools: `confidence_report`, `justification` (CF-17/18).
- **Invariants & telemetry backflow** (`invariants.py`, CF-19..21): safe
  contract-expression evaluation against a state snapshot + violation
  recording; graph traversal from a code node back to its
  contracts/tests. Tools: `check_invariant`, `invariant_violations`,
  `telemetry_to_spec`.

**Section 15 — Decomposition Engine** (`decompose.py`): reuses the
existing HTML walker + interactive-element detector to decompose pages
into `Page`/`ImpliedPage`/`InteractiveElement`/`DerivedTaskHint` nodes.
Tools: `decompose_page`, `page_tree`, `promote_hint_to_task`,
`element_coverage`, `implied_pages_run`, `implied_pages`.

**Section 16 — Test Execution & APM** (`test_orchestration.py`,
`mocking.py`, `mock_server.py`, `apm.py`): test-plan generation over
interactive elements / contracts / transitions / API endpoints / PRD error
scenarios, test execution, coverage-gap reporting, nook-and-corner
testing, unified coverage reports; third-party mock orchestration with a
**real runnable FastAPI mock server** (explicit base-URL override, not
network interception); APM metric ingestion incl. automatic pytest
`--junitxml` timing collection, baselines, regression detection. Tools:
`test_plan`, `run_tests`, `record_test_result`, `test_results`,
`coverage_gaps`, `nook_and_corner_test`, `unified_coverage_report`,
`mock_registry_run`, `mock_registry`, `mock_coverage`, `start_mock_server`,
`stop_mock_server`, `mock_violations`, `record_apm_metric`, `apm_metrics`,
`performance_baseline`, `performance_regressions`.

**Section 17 — System Intelligence** (`subsystems.py`): a static registry
of every subsystem actually built in this codebase, with
`(repo, project) -> int` population queries (callable, not raw Cypher, so
the same test passes against both Neo4j and the in-memory double). Tools:
`subsystem_health`, `subsystem_gaps`, `subsystem_dependency_graph`,
`subsystem_dependency_graph_run`, `population_path`.

**Runtime telemetry ingestion** (`telemetry.py`, CI-15..17): real
OpenTelemetry span ingestion over **OTLP/HTTP with JSON encoding**
(received at `POST /telemetry/otlp`), distinct from test-time APM. Raw
protobuf decoding is deliberately not attempted.

**Virtual filesystem & sandbox** (`cie/tools/view.py`, `edit.py`,
`runner.py`, `blame.py`): jailed `view_file` (line-numbered, with a
graph-joined symbol index, configurable size ceiling), `write_file`,
`write_files_atomic`, `edit_file`, `delete_file`, `run` (subprocess +
cwd jail + hard timeout — `CIE_RUN_ROOT` widens the jail), `blame_history`
(git history joined with task-graph artifacts). Every write keeps the
in-process heuristic symbol index incrementally fresh and re-resolves
callers of unchanged files.

**Heuristic fallback** (`cie/tools/index.py`, `heuristic.py`): when a
graph call fails or returns empty, `ToolService` lazily builds an
in-memory `SymbolIndex` by walking+parsing the project tree, so
`search_symbol`/`file_skeleton`/`view_file` keep working against an
unindexed or partially-indexed tree — same result-shaping code path as
the graph-backed path.

### Task & PRD-hierarchy layer (`tasks.py`, `task_repository.py`, `embedded_task_repository.py`, `hierarchy.py`)
- `AtomicTask` / `AtomicTaskBatch` (pydantic, schema-versioned at ingest),
  with status/attempts write-back, artifacts, repair events, dependency
  cycles validation, coverage validation, API-contract validation.
- `Neo4jTaskRepository` (real write-behind entity cache, `cie.graph_cache`)
  or `EmbeddedTaskRepository` (SQLite, zero-config — same
  `TaskRepository` protocol, same `plan_push` validation code, no Neo4j)
  — `NullTaskRepository` remains available as an explicit opt-out.
- `hierarchy.py`: stores/traverses a PRD tree (Module → Feature →
  Workflow → UseCase → UserStory → `REALIZED_BY` AtomicTask), APOC-free
  Cypher — **Neo4j only**, not yet ported to the embedded backend (its
  three tools — `prd_coverage`/`prd_orphans`/`prd_traceability_chain` —
  call `cie.factory.get_hierarchy_repo` directly). CLI: `hierarchy:push`,
  `hierarchy:children`, `hierarchy:lineage`.

### Three front-ends, one envelope
- **MCP** (`cie.mcp_server` / `cie-mcp`): real Model Context Protocol over
  stdio (or `sse` / `streamable-http`), built with the official `mcp` SDK.
  Each tool's JSON Schema comes from SDK introspection of the bound method
  — one source of truth. Denied-by-policy tools are **never registered**,
  not merely refused. Policies: `forge`/`orchestrator` (read+write),
  `miner`/`inspector` (read-only).
- **HTTP** (`cie.routes.py`): `router` mounted into the host FastAPI app
  (not a separate process). `POST /tools/{tool}` (kwargs in body),
  `GET /tools`, `GET /health`, `GET /schema-version`, plus dedicated
  `POST /tasks`, `GET /tasks/{name}`, `GET /tasks/pending`,
  `POST /hierarchy`, `POST /telemetry/otlp`, etc.
  **Read-only by default, enforced server-side** through the same
  `ToolPolicy` the MCP path uses: write tools and mutating legacy REST
  routes (`POST /tasks`, `POST /code/reload`, `POST /sync/event`,
  `POST /telemetry/otlp`, …) are 403 (`forbidden` envelope kind) unless
  `CIE_HTTP_POLICY=orchestrator` (or `CIE_HTTP_ALLOW_WRITE=1`; also
  `miner` for read-only-by-name). Mutating requests carrying a
  cross-origin `Origin` are rejected even when writes are allowed (the
  CSRF-to-localhost vector), unless the origin is listed in
  `CIE_HTTP_ALLOWED_ORIGINS`. `GET /tools` discovery is filtered to
  match — a read-only caller can't even see a tool it can't call.
- **CLI** (`cie.cli`, 47 commands, `cie index` included): human Rich
  tables by default; every
  command honors `--json` (group-level, before the subcommand) emitting
  the **same** SPEC §0 envelope as the HTTP surface, so an agent can drive
  cie entirely over JSON. Commands mirror the tools above (`search`,
  `node`, `neighbors`, `community`, `communities`, `god`, `stats`,
  `search-symbol`, `view-file`, `callers`, `callees`, `skeleton`,
  `failing-context`, `affected-by`,
  `blame`, `run`, `reindex`, `watch`, `tasks:*`, `hierarchy:*`,
  `coverage:*`, `validate:*`, `schema-version`, `schema:dump`, …).

### Security & determinism notes (from the code)
- File tools jail unconditionally under the project root
  (`cie.tools.view._jail`); `CIE_RUN_ROOT` can *widen* the `run` jail only.
  There is no "disable the jail" option.
- Every edge carries provenance (`extracted_at`/`extractor_version`/
  `source_ref`); confidence is stamped at write time, never invented by
  the pure extractor.
- Per-operation wall-clock timeouts (`cie.timeouts`) bound lock-wait hangs
  that the driver's own timeouts don't — a direct lesson from a real
  2026-08-04 Aura schema-lock incident documented in `cie.timeouts`.
- Citations in GraphRAG are assembled from the graph, never emitted by the
  LLM, so they can't be fabricated mid-generation.

## Two tiers

cie has two tiers, and the split is deliberate — they target two
different audiences:

**Acquisition tier — zero-config, embedded.** One local SQLite file, no
server, nothing to configure (see Quickstart). The full code graph
(search, traversal, call graph, file skeleton, the virtual filesystem,
the heuristic fallback, GraphRAG Q&A) + ~135 tools over MCP/HTTP/CLI.
**No task/QA tracking, no quality-governance layer** (clone/drift
detection, confidence, contracts). This is the tier a solo dev or a
first-time visitor tries — the sharp hook that wins the first star.

**Retention tier — Neo4j-backed.** Every capability, multi-project
namespacing, and the things a team keeps querying every day (not a
one-time "wow"): task/QA traceability (which tasks and tests implement
which code), continuous quality-governance, the PRD hierarchy, and
coverage trending. This is the tier that makes cie worth keeping
installed past week one — the story no pure code graph has.

```python
from pathlib import Path
from cie.config import CieConfig, Neo4jConfig
from cie.factory import build_tool_service_from_config

config = CieConfig(
    project_root=Path("/path/to/your/project"),
    project="my-project",
    neo4j=Neo4jConfig(uri="bolt://localhost:7687", user="neo4j", password="password"),
)
service = build_tool_service_from_config(config)

service.reindex()
print(service.search_symbol("main"))
```

Or over MCP: `cie-mcp /path/to/your/project` (no `--embedded`) — reads
`CIE_NEO4J_*`/`NEO4J_*` env vars, or pass `--neo4j-uri`/`--neo4j-user`/
`--neo4j-password` explicitly.

## Docs

- [Security model](docs/security.md) — the `run` tool's jail, timeout,
  and (optional) container seam, stated as a threat model: what is
  isolated, what is NOT, and which surface refuses what by default.
- [Competitive landscape](docs/competitive-landscape.md) — nearest
  competitors (CodeGraphContext, CodeGraph, Serena, and others), where cie
  differs, and where it's honestly behind.
- [Benchmarks — psf/requests](docs/benchmarks-requests.md) — the same
  methodology re-run on a well-known public repo this project didn't
  write, not a self-referential proof case; a real win and a real recall
  gap, both reported.
- [Benchmarks — urllib3](docs/benchmarks-urllib3.md) — the third dataset,
  fully regenerable from [`scripts/benchmark.sh`](scripts/benchmark.sh).
- [Tool-selection accuracy](docs/tool-selection-accuracy.md) — does
  having 81+ tools instead of ~14 cost an agent selection accuracy?
  Measured, not asserted: 14/14 correct in both conditions, one run —
  the hypothesis that breadth costs accuracy didn't hold up here.
- [Benchmarks](docs/benchmarks.md) — real tool-call/response-size
  measurements against a real codebase, published honestly (including
  where it didn't win).
- [Competitor benchmarks](docs/competitor-benchmarks.md) — the same real
  codebase indexed and queried with CodeGraphContext and Serena actually
  installed and run (not estimated), including a real ambiguous-name
  resolution bug this digging uncovered, diagnosed precisely, and fixed.
- [Adding a language](docs/adding-a-language.md) — a complete, verified
  `LanguageAdapter` for a language cie has never seen, no tree-sitter
  grammar or LSP involved.

## Project layout

```
cie/
  models.py            # NodeKind/Edge/Confidence + all result dataclasses (one source of truth)
  repository.py        # Repository Protocol
  neo4j_repository.py  # Neo4j (Cypher) backend
  in_memory_repository.py  # reference test double + embedded query/traversal logic
  embedded_repository.py   # zero-config SQLite backend
  query.py             # QueryEngine (backend-agnostic orchestration)
  extract.py           # tree-sitter extraction (Python/JS/TS/Java/Go/Rust/C/C++/C#)
  callgraph.py         # pass-2 calls/inheritance edge resolution
  testlink.py          # TESTS edge resolution
  lang_adapter.py      # pluggable language-adapter registry + entry points
  config.py factory.py # bootstrap (Neo4jConfig / CieConfig / build_tool_service*)
  tools/               # ToolService (~135 tools) + jailed fs/run/blame helpers
  mcp_server.py        # real MCP server (cie-mcp)
  routes.py            # FastAPI router (mounted into host app)
  cli.py               # 49-command CLI (Rich tables + --json envelope)
  tool_schema.py tool_policy.py  # typed JSON-Schema + per-agent authorization
  # analysis passes (on-demand, write analysis nodes):
  clone_detect.py perf_analyze.py drift_detect.py metrics.py
  community_detect.py graphrag.py query_plan.py graph_diff.py
  contracts.py test_synthesis.py state_machine.py traceability.py
  semantic_diff.py consensus.py confidence.py justification.py
  invariants.py telemetry.py decompose.py subsystems.py
  sync.py data_model.py api_routes.py source_analysis.py
  test_orchestration.py mocking.py mock_server.py apm.py
  tasks.py task_repository.py hierarchy.py   # task / PRD-hierarchy layer
  envelope.py embed.py graph_cache.py timeouts.py telemetry.py
tests/                # test_standalone_smoke / test_mcp_server / test_embedded_repository
```

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for dev setup, the test suite,
and the design boundary to respect. Looking for a first PR? Start with
the [`good first issue`](https://github.com/kannamma-labs/cie/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
label — each one names a safe entry-point module and acceptance
criteria. CONTRIBUTING.md's "Becoming a second maintainer" section is
the path beyond a one-off PR.

## License

cie is released under the [MIT License](LICENSE).

By contributing, you agree your contributions are licensed under the same
MIT license — see [`CONTRIBUTING.md`](CONTRIBUTING.md).