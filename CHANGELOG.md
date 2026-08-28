# Changelog

All notable changes to **cie — Code Insight Engine** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(with [PEP 440](https://peps.python.org/pep-0440/) pre-release spelling:
`0.1.0a1` is the first alpha, preceding the eventual `0.1.0` stable).

## [Unreleased]

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