# cie — Code Insight Engine

**A code graph that extends to any language, even ones with no LSP and no
tree-sitter grammar — and the only one that also tracks which tasks and
tests actually implement which code.**

Zero-config to try: index a project into a local SQLite file and serve it
to Claude Code, Cursor, or any MCP client in two commands, no server to
run. Point it at Neo4j instead when you need it for a real team/project.

Originally built inside [protobox](https://github.com/arunsoman/protobox)'s
`be-v2` backend and carved out here as its own package — see protobox's
`be-v2/docs/plans/cie-standalone-any-project-plan.md` for the design
history, and [`docs/competitive-landscape.md`](docs/competitive-landscape.md)
for how it compares to CodeGraph, CodeGraphContext, Serena, and others.

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
[`cie/tool_policy.py`](cie/tool_policy.py). Task/QA tracking isn't part of
the zero-config path (see below); everything else is.

See [`docs/benchmarks.md`](docs/benchmarks.md) for what this actually
buys you, measured against a real codebase, including a case where it
didn't help.

## What it is

- **A generic code graph.** Structural extraction (symbols, call graph,
  imports) via pluggable `LanguageAdapter`s — ships with tree-sitter
  support for Python/JS/TS/Java out of the box; add your own adapter for
  any other language (wrapping a compiler's own AST dump, an LSP server,
  or a tree-sitter grammar) via `cie.lang_adapter.register_adapter`, no
  code change to this package required.
- **~123 LLM-callable tools** (`cie.tools.ToolService`) — symbol search,
  call-graph traversal, clone/community/drift detection, quality reports,
  test-intelligence, traceability, and a jailed virtual filesystem
  (`view_file`/`write_file`/`edit_file`/`write_files_atomic`), all
  self-describing (`ToolService.describe()`), exposable as typed
  JSON-Schema tool definitions (`cie.tool_schema`) with per-agent-type
  authorization (`cie.tool_policy`), and servable over the real Model
  Context Protocol (`cie.mcp_server`, `cie-mcp`).
- **A task/PRD-hierarchy layer** (`cie.task_repository`, `cie.hierarchy`)
  for tracking atomic dev/QA tasks and (optionally) a project's PRD
  decomposition tree — Neo4j only; not part of the zero-config embedded
  path (see `cie.embedded_repository.NullTaskRepository`).

## Install

```bash
pip install cie             # core: graph, tools, task/hierarchy layer over Neo4j
pip install "cie[mcp]"      # + the MCP server (cie-mcp) — what most people want
pip install "cie[http]"     # + the HTTP tool-mount / mock server (cie/routes.py)
```

## Two ways to run it

**Zero-config, embedded** — one local SQLite file, no server, nothing to
configure. See Quickstart above. Scope: the full code graph (search,
traversal, call graph, file skeleton, the virtual filesystem) — not task/
QA tracking, not yet the quality-governance layer (clone/drift detection,
confidence scoring, contracts). Good for trying cie or a single-project
local setup.

**Neo4j-backed** — every capability, multi-project namespacing, and what
you want for a real team:

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

- [Competitive landscape](docs/competitive-landscape.md) — nearest
  competitors (CodeGraphContext, CodeGraph, Serena, and others), where cie
  differs, and where it's honestly behind.
- [Growth plan](docs/growth-plan.md) — how CodeGraph reached 47k+ stars in
  under 5 months, and what has to be true of cie before that playbook
  applies here.
- [Benchmarks](docs/benchmarks.md) — real tool-call/response-size
  measurements against a real codebase, published honestly (including
  where it didn't win).

## License

No license file yet — add one before treating this as open for others to
use (public visibility alone doesn't grant reuse rights).
