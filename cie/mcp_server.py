"""Real Model Context Protocol server over cie's existing tool surface.

Closes the single gap named as the highest-leverage next move in
`docs/competitive-landscape.md` and Phase 0 of `docs/growth-plan.md`: cie
did not speak MCP, so it couldn't plug into Claude Code / Cursor / Codex /
any MCP host the turnkey way every competitor in that research does.

Wraps `cie.tools.ToolService` (~121 methods) as MCP tools using the
official `mcp` SDK's `FastMCP` high-level server, filtered by a
`cie.tool_policy.ToolPolicy` so a connecting MCP client only ever sees
— and can only ever call — the tools that policy permits. This is
server-enforced, not left to the client's own settings, matching
`competitive-landscape.md`'s differentiator #4 ("every competitor leaves
write-permission entirely to the MCP client"). Each tool's JSON Schema is
derived by the SDK's own introspection of the bound `ToolService` method
— the exact same type hints `cie.tool_schema` already reads — so there is
one source of truth for a tool's shape, not two schema-translation layers
that could drift.

Requires the optional `mcp` dependency: ``pip install "cie[mcp]"``.

SDK note — version-agnostic: the high-level server class moved between
mcp major versions. **mcp 2.x** ships ``mcp.server.mcpserver.MCPServer``
(``FastMCP`` was renamed to ``MCPServer``; importing ``mcp.server.fastmcp``
raises a stub ``ModuleNotFoundError`` pointing at the migration guide).
**mcp 1.x** ships ``mcp.server.fastmcp.FastMCP`` (and has no
``mcp.server.mcpserver`` at all). Both expose the same surface this module
relies on — ``add_tool(fn, name=, description=)`` / ``list_tools()`` /
``call_tool(name, arguments)`` / ``run(transport=...)`` / ``__init__(name=)``
— so ``_mcp_server_class()`` below prefers 2.x and falls back to 1.x, and
the same code runs against either installed major version. The one
difference callers see is ``call_tool``'s return: 2.x returns a
``CallToolResult`` envelope (``.is_error``/``.content``), 1.x returns a
``list[ContentBlock]`` directly; tests normalize both.
"""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import TYPE_CHECKING

from cie.tool_policy import AgentType, ToolPolicy

if TYPE_CHECKING:
    from typing import Any

    from cie.tools import ToolService

    # The concrete class is mcp.server.mcpserver.MCPServer (mcp 2.x) or
    # mcp.server.fastmcp.FastMCP (mcp 1.x); both share the surface used
    # here. Kept as a loose alias so type-checking resolves on either SDK.
    McpServer = Any

#: Named policies a caller can select via `--policy` — see
#: `cie.tool_policy` for what each actually permits.
#:
#: Canonical names are ``full`` (read+write) and ``readonly`` (read-only).
#: The historical names ``forge``/``orchestrator`` (read+write) and
#: ``miner``/``inspector`` (read-only) are kept as deprecated back-compat
#: aliases — cie is no longer shaped around any one host pipeline, but
#: existing scripts that pass ``--policy forge`` keep working.
_FULL = ToolPolicy(AgentType.FORGE, allow_write=True)
_READONLY = ToolPolicy(AgentType.INSPECTOR, allow_write=False)
POLICIES_BY_NAME: dict[str, ToolPolicy] = {
    # canonical
    "full": _FULL,
    "readonly": _READONLY,
    # deprecated aliases (same permission level as their canonical name)
    "forge": _FULL,
    "orchestrator": _FULL,
    "miner": _READONLY,
    "inspector": _READONLY,
}


def _mcp_server_class():
    """Return the high-level MCP server class for the installed `mcp` SDK.

    Prefer mcp 2.x's ``mcp.server.mcpserver.MCPServer``; fall back to mcp
    1.x's ``mcp.server.fastmcp.FastMCP`` when 2.x isn't installed. Both
    expose the same ``add_tool``/``list_tools``/``call_tool``/``run``/
    ``__init__(name=)`` surface this module uses. Importing lazily (here,
    not at module top) keeps `mcp` optional: this module imports fine with
    no `mcp` installed; only calling ``build_mcp_server`` requires it.
    """
    try:
        from mcp.server.mcpserver import MCPServer  # mcp 2.x

        return MCPServer
    except ModuleNotFoundError:
        from mcp.server.fastmcp import FastMCP  # mcp 1.x

        return FastMCP


def build_mcp_server(service: "ToolService", policy: ToolPolicy, *, name: str = "cie") -> "McpServer":
    """Build an MCP server exposing every tool `policy` permits on
    `service`. Tools `policy` denies are never registered at all — an MCP
    client's own `tools/list` response never names them, not merely
    refuses to run them."""
    server = _mcp_server_class()(name=name)
    for tool_name in sorted(vars(type(service))):
        if tool_name.startswith("_") or tool_name == "describe":
            continue
        attr = vars(type(service))[tool_name]
        if not callable(attr):
            continue
        if not policy.permits(tool_name):
            continue
        method = getattr(service, tool_name)
        doc = (method.__doc__ or "").strip()
        description = doc.splitlines()[0] if doc else tool_name
        server.add_tool(method, name=tool_name, description=description)
    return server


def resolve_policy(name: str) -> ToolPolicy:
    """Look up a policy by its `--policy` name. Raises `ValueError` (not a
    KeyError) with the valid choices, for a clean CLI error message."""
    try:
        return POLICIES_BY_NAME[name]
    except KeyError as exc:
        raise ValueError(
            f"unknown policy {name!r} — choose one of {sorted(POLICIES_BY_NAME)}"
        ) from exc


def _build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="cie-mcp",
        description="Run cie as a Model Context Protocol server (stdio transport by default).",
    )
    parser.add_argument("project_root", type=Path, help="Project root to serve tools against.")
    parser.add_argument("--project", default="", help="cie project namespace (default: unscoped).")
    parser.add_argument(
        "--policy", default="full", choices=sorted(POLICIES_BY_NAME),
        help="Which ToolPolicy to enforce (default: full — read+write). "
             "Read-only clients should use 'readonly'. Historical aliases "
             "'forge'/'orchestrator'/'miner'/'inspector' still work.",
    )
    parser.add_argument(
        "--embedded", action="store_true",
        help="Zero-config: use a local SQLite graph (docs/growth-plan.md "
             "Phase 0) instead of Neo4j — run `cie index PROJECT_ROOT` "
             "first. Task/QA tracking tools are unavailable in this mode.",
    )
    parser.add_argument(
        "--db", type=Path, default=None,
        help="With --embedded: SQLite graph file (default: <project_root>/.cie/graph.db).",
    )
    parser.add_argument("--neo4j-uri", default=None)
    parser.add_argument("--neo4j-user", default=None)
    parser.add_argument("--neo4j-password", default=None)
    parser.add_argument(
        "--transport", default="stdio", choices=["stdio", "sse", "streamable-http"],
        help="MCP transport (default: stdio — what Claude Code/Cursor/Codex expect for a local server).",
    )
    return parser


def main(argv: list[str] | None = None) -> None:
    """CLI entry point — registered as the `cie-mcp` console script."""
    args = _build_arg_parser().parse_args(argv)
    policy = resolve_policy(args.policy)

    if args.embedded:
        from cie.factory import build_tool_service_embedded

        service = build_tool_service_embedded(
            args.project_root, db_path=args.db, project=args.project,
        )
    else:
        from cie.config import CieConfig, Neo4jConfig
        from cie.factory import build_tool_service_from_config

        neo4j = None
        if args.neo4j_uri or args.neo4j_user or args.neo4j_password:
            neo4j = Neo4jConfig(
                uri=args.neo4j_uri or "bolt://localhost:7687",
                user=args.neo4j_user or "neo4j",
                password=args.neo4j_password or "password",
            )
        config = CieConfig(
            project_root=args.project_root,
            project=args.project,
            neo4j=neo4j or Neo4jConfig.from_env(),
        )
        service = build_tool_service_from_config(config)

    server = build_mcp_server(service, policy)
    server.run(transport=args.transport)


if __name__ == "__main__":
    main()
