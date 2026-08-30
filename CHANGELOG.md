# Changelog

All notable changes to **cie — Code Insight Engine** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(with [PEP 440](https://peps.python.org/pep-0440/) pre-release spelling:
`0.1.0a1` is the first alpha, preceding the eventual `0.1.0` stable).

## [Unreleased]

### Added

- **R15 — `cie init`: one-command onboarding.** Detects installed MCP
  clients (Claude Code / Cursor by config presence; Codex detected and
  given the exact TOML snippet — never auto-edited), registers cie's
  stdio server idempotently (existing entries byte-preserved; invalid
  JSON refused, not 'fixed'), writes managed context blocks into
  `AGENTS.md`/`CLAUDE.md` (`cie:init` markers, user content outside
  them untouched, re-run = refresh-in-place), and defaults the client's
  policy to **readonly** — writes are an explicit `--policy full` opt-
  in, not an onboarding side effect. Verified end to end: the
  registered entry, spawned exactly as a client would, handshakes over
  real stdio and lists 85 read tools with zero write tools
  (`scripts/record_init.sh` + `tool-test-lab/dogfood_mcp_stdio_list.py`).
  13 tests.

### Added

- **R16 — the `run`-tool isolation story made explicit.** New
  `docs/security.md` states the threat model precisely: the cwd jail +
  hard-timeout/process-group kill + bounded output that IS there, and
  equally plainly what is NOT (no fs sandbox beyond cwd, no network
  restriction, no container isolation) — plus the surface-by-surface
  matrix of what each caller can reach (HTTP default refuses `run`
  server-side before any process spawns, machine-checked by a test that
  monkeypatches Popen to blow up on any invocation; MCP `readonly`
  never registers it). Optional `CIE_RUN_WRAPPER` container seam —
  documented as convenience, not enforcement, with the pin that absent
  the env var behavior is byte-identical.

### Added

- **R9 — reproducible benchmark harness + a third independent repo.**
  `scripts/benchmark.sh` + `scripts/benchmark_tasks.py` turn the
  benchmark-doc methodology into a script: clone at the pinned commit →
  index → run the three canonical task shapes on BOTH sides (naive
  grep/read vs tool calls), emitting the JSON the docs' tables are
  pasted from — plus the requested token-per-query metric, measured as
  response-payload chars (tokenizer-free, labeled as such — ours gets
  measured, codebase-memory-mcp's "120× fewer tokens" stays a
  vendor-claim). Third dataset: docs/benchmarks-urllib3.md (urllib3 @
  85a8a9cf, 36 files / 667 nodes / 1,307 edges) — wins (receiver-wise
  caller attribution: 12 graph edges vs 28 undifferentiated grep
  matches; 2.24× file-skeleton compression) AND misses (28/40
  unresolved call sites — the known heuristic-recall gap, bigger on
  this repo, published as found). The psf/requests numbers regenerate
  via the same harness and match the published doc.

### Added

- **R8 — `cie export-html`: the shareable artifact.** One static,
  self-contained HTML snapshot of a project's graph, centered on what no
  competitor renders: task→file→test chains (real TESTS edges from the
  index pass), orphan symbols, the atomic-task list, indexed files, and
  a text filter — no server, no auth surface, no network access, opens
  via `file://` (zero external references asserted in tests).
  `scripts/record_export_html.sh` reproduces end to end: clone psf/
  requests at the pinned commit → index → export → headless-Chrome
  screenshots straight from `file://` (committed to `docs/images/`).
  CLI: `cie export-html [PATH] --out FILE [--max-chains N]`.

### Added

- **R11 — streamable-HTTP transport for `cie-mcp`.** The `--transport
  streamable-http` choice is now actually usable end to end: `--host`
  (default `127.0.0.1`) / `--port` (8000) kwargs wired into the SDK's
  HTTP run path, the same server-side `ToolPolicy` applies on every
  transport. Live-verified with the official streamable-http client
  against a real spawned server (`tool-test-lab/dogfood_mcp_http.py`):
  HTTP `tools/list` == exactly the 85-tool inspector prediction, and a
  write attempt is refused server-side (the tool is never registered —
  never trusted to the client). Browser-mode Inspector is the human
  path; the harness is its scriptable twin.

### Added

- **R7 — edge provenance tagging: callers/callees disclose HOW each
  answer was reached.** Every row in `callers`/`callees` carries
  `provenance` — `"graph"` (a persisted, confidence-tagged edge) vs
  `"heuristic-name-match"` (the fallback served it, say so per row) —
  and the envelope carries `resolution` (`CallResolutionStat`, persisted
  by `cie index`/`cie load` in the edge-resolution pass): per-name
  `{total_call_sites, unresolved_call_sites, resolved_edges}`. The
  benchmark docs' honest miss ("resolved 3 of 6 real call sites on
  requests") is now a field in tool output — live-verified on
  psf/requests: `resolution: {total: 19, unresolved: 16, resolved: 3}`
  (name-keyed; see benchmarks-requests.md's reconciliation note).
  `callgraph.resolve_call_edges` gained the same-pass stats companion
  (`resolution_stats`); ground-truth provenance tests pin the labels
  against known-by-inspection truth.

### Added

- **R14 — the PRD-hierarchy store lands on embedded SQLite.** Its last
  Neo4j-only feature is gone: `cie/embedded_hierarchy_repository.py`'s
  `SQLiteHierarchyRepository` implements the same `HierarchyRepository`
  protocol (`push_hierarchy`/`get_children`/`get_lineage`/
  `get_hierarchy_node`/`get_project_tree`) over one local file
  (`.cie/hierarchy.db` beside the other embedded stores). Intentional,
  documented backend differences: one HAS_CHILD edge direction (nothing
  writes the host wide-schema's CHILD_OF on this path) and name-keyed,
  unconditional REALIZED_BY edges (the task layer lives in a separate
  file; Neo4j's write validates them against stored AtomicTask nodes).
  The three hierarchy tools are real `ToolService` methods now — the
  last HTTP-only alias handlers; `HTTP_WRITE_ALIASES` is EMPTY
  (permanently, pinned), `push_hierarchy` joined `WRITE_TOOLS`, and the
  default `cie-mcp --embedded` serves the full PRD tree (opt out with
  `--no-hierarchy` / `hierarchy_tracking=False`; the tools then return
  `unavailable[HIERARCHY_STORE_NOT_CONFIGURED]`, never silent-empties).
  CLI `hierarchy:*` commands also work against embedded now (R2's seam
  extends to the hierarchy repo). 21 tests
  (`tests/test_embedded_hierarchy_repository.py`, B1's bar) + the pinned
  honest-unavailable-when-off test. `cie/hierarchy.py`'s docstring
  pointer to a nonexistent in-memory fake corrected in the same pass.

### Added

- **R5 — the 503 surface shrinks 18 → 5, each with a machine-readable
  reason.** The four protobox-leftover modules
  (`cie/community_detect.py`, `cie/contracts.py`, `cie/state_machine.py`,
  `cie/test_orchestration.py`) no longer import `core.llm` at module
  level: the import (and the `Prompt`/`LlmAgent` definitions it fed)
  moved into the one function per module that actually calls the LLM.
  Everything whose logic was pure now runs standalone —
  `community_detect_run` (label propagation), `community_search`,
  `contracts` (query), `validate_types`, `inject_assertions`,
  `strip_assertions`, `test_plan`, `run_tests`, `test_results`,
  `coverage_gaps`, `nook_and_corner_test`, `unified_coverage_report` —
  no shell LLM dependency at all. The five tools that genuinely need a host-only
  backend (`qa`, `contracts_run`, `state_machine_run`,
  `community_summarize_run`, `decompose_page`) now 503 with
  `error.reason` slugs (`OPTIONAL_BACKEND_MISSING:core` /
  `HOST_PLUGIN_MISSING:decompose-detector`) instead of prose — the error
  envelope grows the optional `reason` field, the conformance
  harness surfaces it, and `tests/test_unavailable_reasons.py`
  pins the registry + a no-quiet-regrowth gate. Found and fixed en
  route by the new bucket scan: `failing_context("")` crashed
  (IsADirectoryError) through the heuristic fallback — now an honest
  validation-hint envelope. Live conformance: 132 tools, **100
  verified / 23 graceful / 5 unavailable / 4 backend-gated / 0
  crashes** (`tool-test-lab/surface_results.json`, fresh artifact).

- **MCP write-side parity for the task/QA layer (roadmap R1).** The six
  task/QA write-back tools — `push_tasks`, `set_task_status`,
  `link_artifact`, `append_repair_events`, `record_coverage`,
  `record_coverage_snapshot` — are real `ToolService` methods now, so
  the default `cie-mcp --embedded` install (which introspects only
  ToolService) serves the full task/QA surface over MCP, instead of it
  being HTTP-only alias handlers. Same kwargs, same envelope shapes,
  same hints as the handlers they replace; they were added to
  `WRITE_TOOLS` in the same commit (`push_hierarchy` stays an HTTP alias
  until its embedded backend lands in R14). The read-only HTTP/MCP
  story is unchanged and pinned by tests — promoted writes are 403 by
  default server-side, per tool. Surface: 126 → **132 tools**, live
  conformance 88 verified / 22 graceful / 18 unavailable-by-design / 4
  backend-gated / **0 crashes** (`tool-test-lab/surface_results.json` —
  fresh artifact, this commit), with an execution check over a real MCP
  stdio session against `.cie/tasks.db`.

### Fixed

- **The CLI answers the zero-config quickstart (roadmap R2).** Query
  commands (`cie files`, `cie search-symbol`, `cie callers`, `cie
  skeleton`, `cie tasks:*`, …) are no longer hardwired to build a Neo4j
  connection: the same engine answers against the local SQLite graph
  `cie index` writes. Selection rule: `--backend` flag › ``CIE_BACKEND``
  env › auto (embedded when a `.cie/graph.db` exists at `--db`/`CIE_DB`/
  cwd, else Neo4j — unchanged for existing Neo4j users). Task commands
  read/write the sibling `.cie/tasks.db` via the existing embedded task
  repository; `hierarchy:*` commands say honestly that the SQLite store
  is roadmap R14 (not a bolt retry-loop); `load`/`watch`/`bootstrap` +
  explicit-embedded carry the embedded-equivalent hint (`cie index`)
  instead of a connection failure. This closes the session-7 leftover
  where the quickstart literally retried `localhost:7687` four times
  before failing. Suite: 171 → 184 (`tests/test_cli.py`, 13 tests, real
  click tree, no Neo4j anywhere).

### Changed

- `cie <query>` commands gained explicit backend selection (`--backend`,
  `--db`, `CIE_BACKEND`, `CIE_DB`) and now fail fast with a `not_found`
  envelope when explicitly pointed at a nonexistent local graph — never
  a silently-empty answer.

## [0.1.0a3] - 2026-08-30 — dogfooded alpha

### Added
- **Repair transaction layer — the propose/apply/verify patch protocol**
  (`cie/patch.py`, tools on `ToolService`): three tools that separate
  *reasoning* from *mutation* in an autonomous repair loop, over one
  immutable first-class **PatchPlan** object (`NodeKind.PATCH_PLAN`):

  - **`propose_patch`** — construct a precise, reviewable repair proposal
    and change NOTHING: carries the exact `old_text`→`new_text` changes a
    unified diff each, the file-content hash the proposal was made
    against, diagnosis + evidence, intended behavior, a counterfactual,
    blast-radius impact resolved server-side from the graph (affected
    symbols, callers, mapped tests), auto-assessed risk when none is
    supplied, and provenance (agent/model/repository revision). The model
    shouldn't have to hand-populate a patch plan; the graph already holds
    most of it.
  - **`apply_patch`** — the ONLY tool that mutates files on this surface,
    guarded by a gate pipeline that all runs BEFORE any byte is written:
    patch still PROPOSED (patches are immutable; REJECTED is terminal),
    every path jailed under the project root, every `old_text` still
    matching CURRENT content exactly-once (a proposal made against revision
    A is rejected — `PATCH_CONTEXT_MISMATCH` — not blindly applied to
    revision B), the plan's own `allowed_files` scope respected
    (`PATCH_SCOPE_VIOLATION`), post-change content still parses
    (Python-exact via `ast.parse`; other languages honestly `skipped`),
    then ONE all-or-nothing write (snapshot + rollback underneath) with an
    immediate re-hash integrity check and the graph re-indexed in the same
    call. Gate failures record the patch's status as REJECTED, with the
    reason — the fix is to re-propose, never to retry stale context.
  - **`verify_patch`** — judge an APPLIED patch on repository evidence
    (never the proposer's claim): is the applied content still in the
    files (exact post-apply hashes), does everything still parse, do the
    net-new imports resolve, does the declared counterfactual actually
    hold (old behavior text gone, new present), which tests cover the
    changed symbols — plus opt-in real `run_tests` over the discovered
    tests and the architecture-check pass. Emits an envelope-only report
    and records the patch's VERIFIED/FAILED status in an append-only
    verification history; the proposing agent never certifies its own fix
    (persist an INDEPENDENT verdict via `record_verdict`).
  - **`get_patch` / `list_patches`** (read-only) — one full plan with its
    audit properties, and the repair history the per-bug metrics
    (first-patch success rate, patches-per-bug) are computed from.
  Patch proposals are immutable (`PROPOSED → APPLIED → VERIFIED|FAILED`,
  with `REJECTED`/`SUPERSEDED` terminal); re-proposing for the same
  failing test supersedes the open proposal instead of mutating it, so
  the FAILED→FAILED→VERIFIED chain per bug stays queryable via
  `list_patches`. Registered in `NodeKind`, the HTTP surface, and
  `tool_policy` (`propose_patch`/`apply_patch`/`verify_patch` are write
  tools; `get_patch`/`list_patches` stay open to read-only policies).
  30 tests (`tests/test_patch_tools.py`), full suite now 123; plus a
  real-MCP round-trip (`scripts/smoke_patch_mcp.py`).
- **`EmbeddedTaskRepository`** (`cie/embedded_task_repository.py`) —
  SQLite-backed, full `TaskRepository` protocol implementation. Task/QA
  tracking (push/list/status, artifacts, repair events, dependency
  traceability, cycle/coverage/API-contract validation) now works on the
  zero-config `cie-mcp --embedded` path, not just Neo4j —
  `build_tool_service_embedded` constructs it by default (`.cie/tasks.db`);
  `task_tracking=False` / `cie-mcp --no-task-tracking` keeps the old
  fail-fast `NullTaskRepository` behavior for callers that want it.
  Reuses `cie.task_repository.plan_push` (the same validation function
  `Neo4jTaskRepository` calls) so acceptance/rejection rules can't drift
  between backends. 17 new tests
  (`tests/test_embedded_task_repository.py`), plus 3 updated/added in
  `tests/test_embedded_repository.py` to cover the new default and the
  `task_tracking=False` opt-out; full suite now 58 tests.
  The separate PRD-hierarchy tree (`cie.hierarchy`) remains Neo4j-only —
  not in scope for this change.
- **Go and Rust extraction** (`cie/extract.py`) — function/method
  declarations, signatures, and receiver/impl-method call resolution,
  each verified against a real tree-sitter parse. Two honest, documented
  gaps for these two languages specifically: no import-edge extraction,
  no docstring extraction. `tests/test_extract_go_rust.py` (9 tests),
  including a call-site test asserting the receiver name specifically
  does NOT leak in as the called name (the real bug the naive version of
  this change would have shipped). Core dependency count: `tree-sitter`
  grammars 4 → 6.
- **Four new test files** for previously-zero-coverage areas:
  `tests/test_clone_detect.py` (quality-governance, 7 tests),
  `tests/test_lang_adapter.py` (the language-adapter registry, 11
  tests), `tests/test_drift_detect.py` (8 tests, including a real
  extracted+resolved circular-dependency fixture, not a hand-built one),
  `tests/test_metrics.py` (6 tests, aggregate scoring against clone/
  drift passes actually run, not hand-inserted analysis nodes). Full
  suite: 4 files/38 tests → 10 files/99 tests.
- **Worked "add a language" example** — `docs/adding-a-language.md` +
  `examples/adapters/toy_regex_adapter.py`, a complete, runnable
  `LanguageAdapter` for a language cie has never seen (no tree-sitter
  grammar, no LSP).
- **A second benchmark dataset** (`docs/benchmarks-requests.md`) — the
  same tool-call/response-size methodology re-run on `psf/requests` (a
  well-known public repo, not this project's own code), addressing a
  fair critique that the first benchmark's proof case was
  self-referential. A real win (43% smaller skeleton on a 1,184-line
  file) and a real miss (the ambiguous-caller query resolved only 3 of 6
  real call sites on this repo, vs. a cleaner precision-only result on
  the first dataset) — published as found, not adjusted.

- **A real demo asset** — `demo.svg` (animated terminal recording,
  embedded at the top of the README) + `demo.cast` (the raw asciinema
  recording), reproducible via `scripts/record_demo.sh`: a real
  `cie-mcp --embedded` server, real MCP stdio JSON-RPC, a real
  `callers("close")` call against `psf/requests`, contrasted with the
  grep a naive agent would run for the same question.

- **A second README hook**: ~121 specific tools doesn't cost an agent
  tool-selection accuracy, measured against cie's own hardest
  near-duplicate cases (`docs/tool-selection-accuracy.md` — 14/14
  correct, full 81-tool surface vs. a 14-tool subset, one run). Added to
  the README right after the benchmark paragraph, and to
  `docs/competitive-landscape.md`'s strengths list.

- Fixed a stale tool-count claim found while re-measuring for this
  entry: the live surface is **126 MCP tools** (83 read-only under
  `--policy inspector`), which is `ToolService`'s 127 public methods
  minus `describe` (the introspection helper that generates the tool
  list, deliberately not itself an LLM tool). Earlier docs variously
  said 121 / 81 — snapshots of different measurement surfaces, now
  labelled as such.

### Fixed
- **The HTTP surface now enforces cie's own `ToolPolicy` — read-only by
  default, server-side.** `POST /tools/{tool}` previously dispatched
  every write tool (`edit_file`, `delete_file`, `apply_patch`, `run`,
  `reindex`, `sync_*`, every `*_run` pass) straight to `ToolService` with
  no authorization — while per-agent-type authorization was advertised as
  a differentiator and `tool_policy.py`'s own docstring named "an
  external HTTP caller" as its intended adopter. Worse, the 17
  task/hierarchy/coverage alias handlers — of which seven mutate
  (`push_tasks`, `set_task_status`, `link_artifact`,
  `append_repair_events`, `record_coverage`, `record_coverage_snapshot`,
  `push_hierarchy`) — are bespoke handlers, not `ToolService` methods, so
  `WRITE_TOOLS` could never have seen them: mutating tools invisible to
  the policy layer. `run_tool` now authorizes every dispatch against
  `_http_policy()` (INSPECTOR by default;
  `CIE_HTTP_POLICY={inspector,miner,orchestrator}` or
  `CIE_HTTP_ALLOW_WRITE=1` to opt into writes), the mutating aliases are
  treated as write tools via the new `routes.HTTP_WRITE_ALIASES`,
  mutating legacy REST routes (`POST /tasks`, `PUT /tasks/{name}/status`,
  artifacts/events, `POST /code/reload`, `POST /sync/event`,
  `POST /telemetry/otlp`) get a `_write_guard` dependency, a cross-origin
  `Origin` on any mutating request is rejected even when writes are
  allowed (the CSRF-to-localhost vector — a `text/plain` POST needs no
  CORS preflight to have side effects; whitelist via
  `CIE_HTTP_ALLOWED_ORIGINS`), `GET /tools` discovery is policy-filtered
  so a read-only caller can't even see a tool it can't call, and
  `POST /api/cie-tools/{tool}` inherits the gate through `run_tool`.
  `err_envelope` gained the `forbidden` error kind (403). Tests: **+16**
  (`tests/test_http_policy.py`); suite 155 → **171**.
- **`cie index` no longer indexes virtualenvs / dependency / cache /
  build / VCS directories** (`cie/extract.py`). Found by dogfooding
  `cie index .` on this repo: the sole directory walk behind `cie
  index`, `cie reindex` and `graph_diff` had zero exclusions, so it
  indexed 1,779 files / ~28k nodes — overwhelmingly `.venv/`
  `site-packages`, including a **stale pip-installed copy of cie
  itself**, which duplicated every blast-radius answer
  (`callers("extract_many")` resolved into
  `.venv/lib/python3.13/site-packages/cie/extract.py` alongside the
  real ones, plus pytest internals). After the fix the same index is
  84 files / 1,554 nodes and answers are repo-only — the healed graph
  now also picks up the new tests below as real callers. Pinned by
  `tests/test_loader_exclusions.py` (5 tests).
- **Optional-backend failures now degrade gracefully instead of
  crashing.** Five modules still import the protobox `core.llm` layer
  (`community_detect`, `contracts`, `graphrag`, `state_machine`,
  `test_orchestration`); because they import lazily per tool call, the
  failure fired at call time on the live read-only surface:
  `coverage_gaps()` crashed with `ModuleNotFoundError: No module named
  'core'` inside a bare "unexpected tool failure; report this"
  envelope. New SPEC §0 error kind **`unavailable`** —
  `cie/envelope.py` registers it, the HTTP tool mount maps it to 503,
  and `ToolService._guard` (the funnel behind every MCP tool result)
  and the HTTP dispatch both emit it — an expected property of the
  standalone installation, not a reportable crash. Full `core.llm`
  decoupling remains open: affected tools honestly return
  `unavailable` rather than pretending. Found by dogfooding; pinned by
  `tests/test_optional_dependency_envelope.py` (9 tests incl. a replay
  of the exact live crash through the real `ToolService.coverage_gaps()`).
  Suite: 129 → **143 tests, all passing**.
- **`get_task` / `list_pending_tasks` / `task_dependency_closure` /
  `blame_history` crashed on the live MCP surface** (`ProgrammingError:
  SQLite objects created in a thread can only be used in that same
  thread`): cie-mcp builds `EmbeddedTaskRepository`'s connection once at
  startup (main thread) and runs tool handlers in anyio worker threads.
  In-process tests never caught it because they run everything in the
  main thread. Fix: `_ThreadSafeSQLite` wrapper
  (`check_same_thread=False` + a lock around statement/commit critical
  sections — transaction boundaries, not just statements, are what
  breaks). Pinned by `tests/test_task_repo_threading.py` (4).
- **Answer-correctness fixes, found by an exact ground-truth suite**
  (`tests/test_graph_semantics_ground_truth.py`, 8 — a fixture project
  where every true caller/callee/blast-radius member is known by
  construction and tools are compared against those sets, not just
  checked for `ok`):
  - `file_skeleton("app.py")` returned `test_app.py`'s symbols: path
    matching was SUBSTRING. Now exact or proper path suffix in both
    backends (`InMemoryRepository` + `Neo4jRepository`, mirror-implemented).
  - `affected_by` (blast radius, the repair agent's actual question)
    omitted depending TEST files: the traversal's allowed-edge set lacked
    the testlink `TESTS` edges, and the substring seed pre-visited the
    test file's nodes so they silently vanished from results. Both
    backends updated: seeds are exact/path-suffix, `TESTS` edges now
    participate — the test file appears in the blast radius of the code
    it exercises, as the project's own headline promises.
  - `qa`'s lazy `core.graphrag` import sat *before* its guard (escaped
    as a raw MCP error, not a SPEC envelope); `decompose_page`'s missing
    optional plugin raised bare `RuntimeError` → typed
    `DetectorUnavailable`, both now surface as `kind=unavailable`.
  Post-fix full-surface conformance re-run (126 tools, live MCP server):
  **85 verified / 19 graceful / 18 unavailable-by-design / 4
  backend-gated (Neo4j) — 0 crashes.** Suite: 143 → **155 tests, all
  passing**.

### Changed
- README leads with a one-sentence hook (task/QA traceability) instead
  of a five-capability list; `docs/competitive-landscape.md` now leads
  with "where cie genuinely excels" instead of the competitor comparison
  table. `CONTRIBUTING.md` gained a "becoming a second maintainer"
  section.

## [0.1.0a2] - 2026-08-28 — corrected alpha

### Fixed
- **The MCP server now runs on both mcp 1.x and 2.x.** `0.1.0a1`'s
  `build_mcp_server` targeted only `mcp.server.fastmcp.FastMCP` (the mcp
  1.x class); on CI — which installs `mcp>=2` — `mcp.server.fastmcp` is a
  stub that raises `ModuleNotFoundError` (in mcp 2.x, `FastMCP` was renamed
  to `MCPServer` at `mcp.server.mcpserver`), so every `test_mcp_server`
  test failed. `build_mcp_server` is now version-agnostic: it prefers mcp
  2.x's `MCPServer` and falls back to mcp 1.x's `FastMCP`, and the
  `call_tool` test normalizes the two return shapes (2.x `CallToolResult`
  envelope vs 1.x `list[ContentBlock]`). Verified on both: 38/38 tests pass
  on mcp 1.x *and* mcp 2.x, plus a real stdio JSON-RPC handshake on each
  (`tools/list` under `--policy readonly` → 81 tools, write tools absent).
- **Python 3.10 actually works.** `cie/data_model.py` did `import tomllib`
  unconditionally, but `tomllib` is Python 3.11+ stdlib, so the module
  failed to import on 3.10 — contradicting `requires-python = ">=3.10"`.
  Now uses a `tomli` backport fallback (`tomli; python_version < "3.11"`
  conditional dependency). Verified: 38/38 tests pass on Python 3.10.20.

## [0.1.0a1] - 2026-08-28 — first alpha

> ⚠️ **Superseded by 0.1.0a2.** This tag's MCP server only ran on mcp 1.x;
  on mcp 2.x (what `pip install "cie[mcp]"` resolves to today) the tests
  fail to import. Use 0.1.0a2.

First (alpha) release of cie — Code Insight Engine: a pluggable,
language-agnostic code graph + LLM tool surface (MCP / HTTP / CLI) with an
embedded SQLite or Neo4j backend.

### Added
- **Generic code graph** with pluggable `LanguageAdapter`s — tree-sitter
  Python / JS / TS / Java out of the box; register your own for any
  language (a compiler AST dump, an LSP server, or a tree-sitter grammar)
  via `cie.lang_adapter.register_adapter` or the `cie.language_adapters`
  entry-point group, no `cie/` code change required.
- **Two-pass loader**: structural extraction → call-graph / inheritance /
  `TESTS`-edge resolution with `EXTRACTED`/`INFERRED`/`AMBIGUOUS`
  confidence tags + provenance (`extracted_at`/`extractor_version`/
  `source_ref`).
- **`ToolService`** — ~121 LLM-callable tools: symbol search, call-graph
  traversal, clone / community / drift detection, quality reports,
  GraphRAG Q&A (citations assembled from the graph, never LLM-fabricated),
  confidence / traceability, and a jailed virtual filesystem.
- **Three front-ends sharing one SPEC §0 envelope**: MCP (`cie-mcp`),
  HTTP (`cie.routes`), CLI (`cie`).
- **Three backends behind one `Repository` protocol**: Neo4j, in-memory
  (the reference test double), and zero-config embedded SQLite
  (`cie index` / `cie-mcp --embedded`).
- **Task / PRD-hierarchy layer** (Neo4j-backed; `NullTaskRepository` on the
  embedded path) — the team retention tier (which tasks/tests implement
  which code, quality-governance, coverage trending).
- **Project hygiene**: MIT `LICENSE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`
  (Contributor Covenant 2.1), `SECURITY.md`, this `CHANGELOG.md`,
  `CITATION.cff`, `.github/` issue + PR templates, a CI workflow (pytest
  across Python 3.10–3.13 + a real MCP stdio handshake smoke check), and
  Dependabot config.
- **`tests/test_tool_surface_invariants.py`** — pins the MCP / HTTP / CLI
  boundary so the three front-ends can't silently drift.

### Changed
- Node URN scheme `urn:protobox:` → `urn:cie:`; RDF export prefix `prb:` →
  `cie:`. One-time identity migration — cie only *emits* URNs (nothing
  parses them back), so no read-side compatibility shim; external
  consumers keyed on the old form must reindex.
- On-disk graph cache moved to a cie-owned path
  `<workspace>/<project>/.cie/graph_cache.db` (`CIE_WORKSPACE_ROOT`).
  `FORGE_WORKSPACE_ROOT` is still read as a deprecated back-compat alias.
- `cie.tasks.GraphBaseModel` / `bind_link` are now cie's own standalone
  definitions — the soft `core.graph.base` import that flipped the
  cie↔host-project dependency direction is gone; the package installs and
  runs with no host project on the path.
- `cie-mcp --policy` canonical names are `full` (read+write) and
  `readonly` (read-only); the historical `forge`/`orchestrator`/`miner`/
  `inspector` are kept as deprecated back-compat aliases. Default is
  `full`.
- `README.md` rewritten: the first line is a hook; the task/QA layer is
  framed as the **Neo4j/team retention tier** ("Two tiers") rather than a
  zero-config feature.
- `pyproject.toml`: MIT license, author, keywords, classifiers, project
  URLs, and a `testpaths = ["tests"]` pytest config (so bare `pytest`
  doesn't mis-collect the `cie/test_*.py` *feature modules*).

### Fixed
- **The MCP server now actually runs** (mcp 1.x only — see 0.1.0a2 for
  the cross-version fix). `build_mcp_server` targeted
  `mcp.server.mcpserver.MCPServer`, a class absent from the mcp 1.x wheel
  on the developer's machine; re-targeted to mcp 1.x's
  `mcp.server.fastmcp.FastMCP`. Verified with a real stdio JSON-RPC
  handshake on mcp 1.x (`initialize` + `tools/list` under
  `--policy readonly` → 81 tools, every write tool correctly absent).
  ⚠️ This broke on mcp 2.x (where `fastmcp` is a stub and `MCPServer` is the
  real class); fixed in 0.1.0a2.

### Removed
- Cut the be-v2-hardwired `resolve_api_route` / `api_call_sites` from
  `ToolService`, `cie.routes.TOOLS`, and the CLI (`resolve-route` /
  `api-call-sites`). The `cie.api_routes` module remains as an internal
  helper for `drift_detect` (CI-11) and `test_orchestration` (API-endpoint
  test plans); full adapter-ization is deferred.

### Security
- File tools jail unconditionally under the project root
  (`cie.tools.view._jail`); `CIE_RUN_ROOT` can *widen* the `run` jail
  only. There is **deliberately no "disable the jail" toggle**.
- MCP policy enforcement: a denied tool is **never registered** on the
  server, so a client's `tools/list` never even names it — not merely
  refused at call time.

[Unreleased]: https://github.com/kannamma-labs/cie/compare/v0.1.0a2...HEAD
[0.1.0a2]: https://github.com/kannamma-labs/cie/releases/tag/v0.1.0a2
[0.1.0a1]: https://github.com/kannamma-labs/cie/releases/tag/v0.1.0a1