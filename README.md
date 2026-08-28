# cie — Code Insight Engine

A pluggable, language-agnostic code-graph, task-tracking, and LLM-tool
surface backed by Neo4j. Originally built inside
[protobox](https://github.com/arunsoman/protobox)'s `be-v2` backend and
carved out here as its own package — see protobox's
`be-v2/docs/plans/cie-standalone-any-project-plan.md` for the design
history and rationale.

## What it is

- **A generic code graph.** Structural extraction (symbols, call graph,
  imports) via pluggable `LanguageAdapter`s — ships with tree-sitter
  support for Python/JS/TS/Java out of the box; add your own adapter for
  any other language (wrapping a compiler's own AST dump, an LSP server,
  or a tree-sitter grammar) via `cie.lang_adapter.register_adapter`, no
  code change to this package required.
- **~120 LLM-callable tools** (`cie.tools.ToolService`) — symbol search,
  call-graph traversal, clone/community/drift detection, quality reports,
  test-intelligence, traceability, and a jailed virtual filesystem
  (`view_file`/`write_file`/`edit_file`/`write_files_atomic`), all
  self-describing (`ToolService.describe()`) and exposable as typed
  JSON-Schema tool definitions (`cie.tool_schema`) with per-agent-type
  authorization (`cie.tool_policy`).
- **A task/PRD-hierarchy layer** (`cie.task_repository`, `cie.hierarchy`)
  for tracking atomic dev/QA tasks and (optionally) a project's PRD
  decomposition tree.

## Install

```bash
pip install cie
# or, for the HTTP tool-mount / mock server:
pip install "cie[http]"
```

Needs a Neo4j instance (5.x) — see `cie.config.Neo4jConfig`/`CieConfig`
for connecting explicitly (no env vars required) or via `Neo4jConfig.from_env()`.

## Quickstart

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

service.reindex()                       # index the project
print(service.search_symbol("main"))     # query it
```

## Docs

- [Competitive landscape](docs/competitive-landscape.md) — nearest
  competitors (CodeGraphContext, CodeGraph, Serena, and others), where cie
  differs, and where it's honestly behind.

## License

No license file yet — add one before treating this as open for others to
use (public visibility alone doesn't grant reuse rights).
