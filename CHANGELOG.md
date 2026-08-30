# Changelog

All notable changes to **cie — Code Insight Engine** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(with [PEP 440](https://peps.python.org/pep-0440/) pre-release spelling:
`0.1.0a1` is the first alpha, preceding the eventual `0.1.0` stable).

## [Unreleased]

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

[Unreleased]: https://github.com/arunsoman/cie/compare/v0.1.0a2...HEAD
[0.1.0a2]: https://github.com/arunsoman/cie/releases/tag/v0.1.0a2
[0.1.0a1]: https://github.com/arunsoman/cie/releases/tag/v0.1.0a1