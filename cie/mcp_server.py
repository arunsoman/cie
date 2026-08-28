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

SDK note: this targets `mcp.server.fastmcp.FastMCP`, the high-level server
shipped by current `mcp` SDK versions (``add_tool`` / ``list_tools`` /
``call_tool`` / ``run(transport=...)``). An earlier revision targeted a
``mcp.server.mcpserver.MCPServer`` class that does not exist in any
released `mcp` wheel; ``FastMCP`` is the real, supported equivalent.
"""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import TYPE_CHECKING

from cie.tool_policy import AgentType, ToolPolicy

if TYPE_CHECKING:
    from mcp.server.fastmcp import FastMCP

    from cie.tools import ToolService

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


def build_mcp_server(service: "ToolService", policy: ToolPolicy, *, name: str = "cie") -> "FastMCP":
    """Build a `FastMCP` server exposing every tool `policy` permits on
    `service`. Tools `policy` denies are never registered at all — an MCP
    client's own `tools/list` response never names them, not merely
    refuses to run them."""
    from mcp.server.fastmcp import FastMCP

    server = FastMCP(name=name)
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
