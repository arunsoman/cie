"""Tests for cie.mcp_server — the real MCP protocol adapter, closing the
gap named in docs/competitive-landscape.md.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

pytest.importorskip("mcp", reason="mcp is an optional dependency (pip install cie[mcp])")

from cie.config import CieConfig, Neo4jConfig
from cie.factory import build_tool_service_from_config
from cie.mcp_server import POLICIES_BY_NAME, build_mcp_server, resolve_policy
from cie.tool_policy import WRITE_TOOLS


class _FakeService:
    def get_task(self, name: str) -> dict:
        """Fetch one atomic task by name."""
        return {"ok": True, "name": name}

    def write_file(self, path: str, content: str) -> dict:
        """Create or overwrite a file."""
        return {"ok": True, "path": path}

    def describe(self) -> dict:  # excluded, same rule as ToolService.describe()
        return {}

    def _private(self) -> None:
        pass


def _run(coro):
    return asyncio.run(coro)


def test_resolve_policy_known_and_unknown():
    assert resolve_policy("inspector") is POLICIES_BY_NAME["inspector"]
    with pytest.raises(ValueError, match="unknown policy"):
        resolve_policy("nonexistent")


def test_inspector_policy_never_registers_write_tools():
    server = build_mcp_server(_FakeService(), POLICIES_BY_NAME["inspector"])
    tools = _run(server.list_tools())
    names = {t.name for t in tools}
    assert names == {"get_task"}
    assert "write_file" not in names
    assert "describe" not in names
    assert "_private" not in names


def test_forge_policy_registers_write_tools():
    server = build_mcp_server(_FakeService(), POLICIES_BY_NAME["forge"])
    tools = _run(server.list_tools())
    names = {t.name for t in tools}
    assert names == {"get_task", "write_file"}


def test_call_tool_dispatches_to_the_real_method():
    server = build_mcp_server(_FakeService(), POLICIES_BY_NAME["forge"])
    result = _run(server.call_tool("get_task", {"name": "t-1"}))
    # mcp 2.x's MCPServer.call_tool returns a CallToolResult envelope
    # (.is_error, .content: list[ContentBlock]); mcp 1.x's FastMCP.call_tool
    # returns the list[ContentBlock] directly. Normalize both so the test
    # passes against either installed major version of the SDK.
    if isinstance(result, list):
        blocks, is_error = result, False
    else:
        blocks = getattr(result, "content", None) or []
        is_error = getattr(result, "is_error", False)
    assert is_error is False
    assert blocks, "call_tool returned no content blocks"
    text = getattr(blocks[0], "text", None)
    assert text is not None
    assert '"name": "t-1"' in text


def test_denied_tool_is_not_even_registered_not_just_refused():
    """A read-only client's tools/list response should never NAME a write
    tool at all — this is the point (competitive-landscape.md's
    differentiator #4), not just that calling it would fail."""
    server = build_mcp_server(_FakeService(), POLICIES_BY_NAME["inspector"])
    tools = _run(server.list_tools())
    assert all(t.name != "write_file" for t in tools)


def test_real_toolservice_registers_cleanly_with_no_introspection_errors(tmp_path):
    """Regression guard: every one of ToolService's ~123 real method
    signatures (Optional[list], dict, defaults, etc.) must be introspectable
    by the MCP SDK's own schema generation with no errors."""
    config = CieConfig(
        project_root=tmp_path,
        neo4j=Neo4jConfig(uri="bolt://localhost:7687", user="neo4j", password="password"),
    )
    service = build_tool_service_from_config(config)

    forge_server = build_mcp_server(service, POLICIES_BY_NAME["forge"])
    all_tools = _run(forge_server.list_tools())
    assert len(all_tools) > 100

    inspector_server = build_mcp_server(service, POLICIES_BY_NAME["inspector"])
    read_only_tools = _run(inspector_server.list_tools())
    read_only_names = {t.name for t in read_only_tools}
    assert "write_file" not in read_only_names
    assert "view_file" in read_only_names

    # The bug this test exists to catch: write_files_atomic (added after
    # WRITE_TOOLS was first written) must be classified as a write tool —
    # confirmed missing once, fixed, guarded here so it can't regress.
    assert "write_files_atomic" in WRITE_TOOLS
    assert "write_files_atomic" not in read_only_names


# ---------------------------------------------------------------------------
# R11 — streamable-HTTP transport wiring
# ---------------------------------------------------------------------------


def test_main_passes_transport_host_port_to_the_server_run(monkeypatch, tmp_path):
    """The `--transport streamable-http` flag (plus --host/--port) must
    reach MCPServer.run as kwargs — the R11 parser already accepted the
    transport choice; this pins the wiring so HTTP kwargs (host/port)
    can't silently detach from the flags."""
    import cie.mcp_server as mcp_server

    captured: dict = {}

    def fake_run(self, transport="stdio", **kwargs):
        captured["transport"] = transport
        captured["kwargs"] = kwargs

    monkeypatch.setattr(
        mcp_server, "build_mcp_server",
        lambda service, policy, name="cie": mcp_server._mcp_server_class()(name=name),
    )
    monkeypatch.setattr(mcp_server._mcp_server_class(), "run", fake_run)
    from cie.mcp_server import main

    main([str(tmp_path), "--embedded", "--transport", "streamable-http",
          "--host", "0.0.0.0", "--port", "9001"])
    assert captured["transport"] == "streamable-http"
    assert captured["kwargs"] == {"host": "0.0.0.0", "port": 9001}


def test_main_stdio_run_takes_no_extra_kwargs(monkeypatch, tmp_path):
    import cie.mcp_server as mcp_server

    captured: dict = {}

    def fake_run(self, transport="stdio", **kwargs):
        captured["transport"] = transport
        captured["kwargs"] = kwargs

    monkeypatch.setattr(
        mcp_server, "build_mcp_server",
        lambda service, policy, name="cie": mcp_server._mcp_server_class()(name=name),
    )
    monkeypatch.setattr(mcp_server._mcp_server_class(), "run", fake_run)
    from cie.mcp_server import main

    main([str(tmp_path), "--embedded"])
    assert captured == {"transport": "stdio", "kwargs": {}}
