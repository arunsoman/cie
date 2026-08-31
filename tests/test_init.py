"""R15 — `cie init` one-command onboarding: client detection, guarded
config merges, managed context blocks, idempotence.

The verify bar (roadmap): fresh clone -> `cie init` -> the client lists
cie's tools with zero manual config. The client side of that is proven
by the stdio handshake path in CI's smoke step + R11's HTTP harness —
these tests pin the init side: WHAT registration writes, that it never
touches existing config, and that re-runs are no-ops.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

from cie.init import (
    BEGIN,
    END,
    _server_command,
    _server_entry,
    detect_clients,
    run_init,
)


@pytest.fixture()
def home(tmp_path):
    h = tmp_path / "home"
    h.mkdir()
    return h


@pytest.fixture()
def project(tmp_path):
    p = tmp_path / "proj"
    p.mkdir()
    (p / "app.py").write_text("def alpha():\n    return 1\n")
    (p / ".mcp.json").unlink(missing_ok=True)
    return p


def test_detect_clients_by_config_presence(home, project):
    assert detect_clients(home, project) == []
    (home / ".claude.json").write_text("{}")
    assert "claude-code" in detect_clients(home, project)
    (home / ".cursor").mkdir()
    assert "cursor" in detect_clients(home, project)
    (home / ".codex").mkdir()
    assert "codex" in detect_clients(home, project)


def test_init_registers_claude_code_project_mcp_json(home, project):
    report = run_init(project, home, clients=["claude-code"])
    data = json.loads((project / ".mcp.json").read_text())
    entry = data["mcpServers"]["cie"]
    # spawn-robust: never a bare name the client's PATH has to resolve
    assert Path(entry["command"]).is_absolute()
    assert entry["args"][-1] == "readonly"
    assert str(project) in entry["args"]
    assert entry["args"][-4:-2] == ["--backend", "embedded"]


def test_server_command_prefers_absolute_script(monkeypatch):
    """Resolution order 1: the console script on PATH resolves to its
    ABSOLUTE path (spawnable by any client environment, PATH-free)."""
    monkeypatch.setattr("cie.init.shutil.which", lambda name: "/opt/bin/cie-mcp")
    command, prefix = _server_command()
    assert (command, prefix) == ("/opt/bin/cie-mcp", [])


def test_server_command_falls_back_to_module_when_not_on_path(monkeypatch):
    """Resolution order 2 (the common venv case): bare `cie-mcp` is NOT
    on the client's PATH — the entry carries the running interpreter +
    `-m cie.mcp_server`, which needs no PATH at all."""
    monkeypatch.setattr("cie.init.shutil.which", lambda name: None)
    command, prefix = _server_command()
    assert command == sys.executable
    assert prefix == ["-m", "cie.mcp_server"]
    # and the generated entry composes it correctly:
    entry = _server_entry(Path("/proj"), "readonly")
    assert entry["command"] == sys.executable
    assert entry["args"][:2] == ["-m", "cie.mcp_server"]
    assert entry["args"][2:] == [
        "/proj", "--backend", "embedded", "--policy", "readonly",
    ]


def test_init_defaults_to_readonly_policy(home, project):
    """The default-safe rule: writes require an explicit --policy full —
    a client the user didn't consciously grant write powers doesn't get
    them from onboarding."""
    run_init(project, home, clients=["claude-code"])
    data = json.loads((project / ".mcp.json").read_text())
    assert "readonly" in data["mcpServers"]["cie"]["args"]


def test_init_never_touches_existing_servers_in_config(home, project):
    cfg = project / ".mcp.json"
    cfg.write_text(json.dumps({
        "mcpServers": {"other-server": {"command": "uvx", "args": ["x"]}},
        "customKey": {"keep": True},
    }))
    run_init(project, home, clients=["claude-code"])
    data = json.loads(cfg.read_text())
    assert data["mcpServers"]["other-server"] == {"command": "uvx", "args": ["x"]}
    assert data["customKey"] == {"keep": True}
    assert "cie" in data["mcpServers"]


def test_init_refuses_to_edit_invalid_json_config(home, project):
    cfg = project / ".mcp.json"
    cfg.write_text("{not json")
    report = run_init(project, home, clients=["claude-code"])
    assert any("refusing" in s for s in report.skipped)
    assert cfg.read_text() == "{not json"  # byte-identical


def test_init_is_idempotent(home, project):
    first = run_init(project, home, clients=["claude-code"])
    assert any("registered" in a for a in first.actions)
    second = run_init(project, home, clients=["claude-code"])
    assert any("already registered" in s for s in second.skipped)
    assert not any("registered '" in a and "already" not in a
                   for a in second.actions)


def test_init_cursor_goes_to_home_config(home, project):
    run_init(project, home, clients=["cursor"])
    assert (home / ".cursor" / "mcp.json").is_file()
    data = json.loads((home / ".cursor" / "mcp.json").read_text())
    assert data["mcpServers"]["cie"]["args"][0] == str(project)


def test_init_codex_is_detected_never_auto_edited(home, project):
    (home / ".codex").mkdir()
    codex_cfg = home / ".codex" / "config.toml"
    codex_cfg.write_text("[other]\nkey = 1\n")
    report = run_init(project, home, clients=["codex"])
    # no silent TOML edits in v1 — the exact snippet is printed instead
    assert codex_cfg.read_text() == "[other]\nkey = 1\n"
    assert any("codex" in h and "mcp_servers." in h
               for h in report.human_actions)


def test_init_writes_managed_context_blocks_neither_clobbering_user_content(
    home, project,
):
    (project / "AGENTS.md").write_text("# MY NOTES\ndo not lose me\n")
    run_init(project, home, clients=["claude-code"])
    agents = (project / "AGENTS.md").read_text()
    assert "do not lose me" in agents
    assert "## cie" in agents and BEGIN in agents and END in agents
    assert (project / "CLAUDE.md").is_file()


def test_init_context_block_refresh_is_replace_in_place(home, project):
    run_init(project, home, clients=["claude-code"])
    (project / "AGENTS.md").write_text(
        (project / "AGENTS.md").read_text().replace("readonly", "WRONG")
    )
    run_init(project, home, clients=["claude-code"])
    text = (project / "AGENTS.md").read_text()
    assert "WRONG" not in text  # only the managed block was refreshed
    assert text.count(BEGIN) == 1


def test_init_no_context_flag_skips_context_files(home, project):
    run_init(project, home, clients=["claude-code"], context=False)
    assert not (project / "AGENTS.md").exists()
    assert not (project / "CLAUDE.md").exists()


def test_registered_entry_passes_the_real_client_handshake(home, project):
    """The R15 verify's cie-side half: the registered entry IS the server
    a client would spawn — run it over real stdio and list tools. Since
    the native-compat fix (2026-08-31) the entry is spawned AS WRITTEN:
    no test-side compensation for a bare command the client's PATH
    can't resolve — the entry itself is spawn-robust."""
    import asyncio
    import os

    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client

    pytest.importorskip("mcp")
    run_init(project, home, clients=["claude-code"])
    entry = json.loads((project / ".mcp.json").read_text())["mcpServers"]["cie"]
    env = {**os.environ, "PYTHONPATH": os.environ.get("PYTHONPATH", "")}

    async def handshake():
        params = StdioServerParameters(
            command=entry["command"], args=entry["args"], env=env,
        )
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()
                tools = await session.list_tools()
                return {t.name for t in tools.tools}

    tools = asyncio.run(handshake())
    assert "search_symbol" in tools      # reads: always visible
    assert "push_tasks" not in tools     # default-safe: writes hidden by policy
    assert "write_file" not in tools     # and the client sees exactly that


def test_init_warns_when_no_client_detected(home, project):
    report = run_init(project, home, clients=[], context=False)
    assert report.human_actions, "zero targets must produce the honest guidance"