"""One storage-backend selection rule, every front-end (2026-08-31).

The motto this pins: **least friction to users and their existing
infrastructure** — auto by default (no forced choice), one explicit
`--backend` knob only when the user cares, existing env vars
(`CIE_BACKEND`, `NEO4J_*`, `CIE_NEO4J_*`) respected, and every old
spelling (`cie-mcp --embedded`) a permanent working alias.

Three layers, one rule:
  - `cie.config.resolve_backend` — the pure rule (this file's unit tests)
  - the `cie` CLI — thin adapter, incl. the explicit-auto regression
    (in tests/test_cli.py::TestBackendSelection)
  - `cie-mcp` — same rule at server startup, with a stderr-only
    diagnostic line (stdout is the JSON-RPC channel — must stay clean)
"""

from __future__ import annotations

import sys

import pytest

from cie.config import resolve_backend


# ---------------------------------------------------------------------------
# The pure rule
# ---------------------------------------------------------------------------


class TestResolveBackend:
    def test_flag_beats_env_and_auto(self, tmp_path, monkeypatch):
        db = tmp_path / "graph.db"
        db.write_text("")
        monkeypatch.setenv("CIE_BACKEND", "neo4j")
        assert resolve_backend("embedded", db_path=db) == "embedded"
        monkeypatch.setenv("CIE_BACKEND", "embedded")
        assert resolve_backend("neo4j", db_path=db) == "neo4j"

    def test_env_beats_alias_and_auto(self, tmp_path, monkeypatch):
        db = tmp_path / "graph.db"
        db.write_text("")
        monkeypatch.setenv("CIE_BACKEND", "neo4j")
        assert resolve_backend(embedded_flag=True, db_path=db) == "neo4j"

    def test_embedded_alias_beats_auto(self, tmp_path, monkeypatch):
        monkeypatch.delenv("CIE_BACKEND", raising=False)
        # no graph.db -> auto would say neo4j; the alias still says embedded
        assert resolve_backend(embedded_flag=True, db_path=tmp_path / "nodb") == "embedded"

    def test_auto_prefers_existing_graph_file(self, tmp_path, monkeypatch):
        monkeypatch.delenv("CIE_BACKEND", raising=False)
        db = tmp_path / "graph.db"
        assert resolve_backend(db_path=db) == "neo4j"
        db.write_text("")
        assert resolve_backend(db_path=db) == "embedded"

    def test_explicit_auto_flag_falls_through_to_detection(self, tmp_path, monkeypatch):
        """The bug this centralization fixed: 'auto' is truthy — a naive
        `if opt: return opt` returned the string itself and callers read
        it as neo4j. 'auto' must MEAN auto."""
        monkeypatch.delenv("CIE_BACKEND", raising=False)
        db = tmp_path / "graph.db"
        assert resolve_backend("auto", db_path=db) == "neo4j"
        db.write_text("")
        assert resolve_backend("auto", db_path=db) == "embedded"

    def test_env_is_tolerant_not_fatal(self, tmp_path, monkeypatch):
        """A stray CIE_BACKEND value never crashes a run — it falls to
        auto (least friction; garbage is not a reason to die)."""
        monkeypatch.setenv("CIE_BACKEND", "sqlite3")
        db = tmp_path / "graph.db"
        assert resolve_backend(db_path=db) == "neo4j"
        db.write_text("")
        assert resolve_backend(db_path=db) == "embedded"

    def test_explicit_env_value_can_be_passed_in(self, tmp_path, monkeypatch):
        """Callers that already read their own env pass it in — the rule
        never re-reads the environment behind their back."""
        monkeypatch.setenv("CIE_BACKEND", "neo4j")
        assert resolve_backend(env="embedded") == "embedded"


# ---------------------------------------------------------------------------
# cie-mcp: same rule at server startup + the stderr-only diagnostic
# ---------------------------------------------------------------------------


def _fake_server_run(monkeypatch):
    """The test_mcp_server.py pattern: no real server, capture transport."""
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
    return captured


def test_mcp_auto_serves_an_existing_index(tmp_path, monkeypatch, capsys):
    """No flags at all + an indexed project = embedded (the quickstart
    contract, now the server's too) — and the choice is STATED on stderr
    while stdout (the JSON-RPC channel for stdio) stays byte-clean."""
    import cie.mcp_server as mcp_server

    _fake_server_run(monkeypatch)
    (tmp_path / ".cie").mkdir()
    (tmp_path / ".cie" / "graph.db").write_text("")

    monkeypatch.delenv("CIE_BACKEND", raising=False)
    mcp_server.main([str(tmp_path)])
    captured = capsys.readouterr()
    assert "backend=embedded" in captured.err
    assert captured.out == ""


def test_mcp_explicit_backend_beats_auto(tmp_path, monkeypatch, capsys):
    """--backend neo4j wins even when an embedded index exists — the
    explicit knob is always honored (and Neo4j construction is faked, so
    the test never opens a socket)."""
    import cie.mcp_server as mcp_server

    built: dict = {}

    class _FakeCfg:
        def __init__(self, project_root, project, neo4j):
            self.project_root, self.project, self.neo4j = project_root, project, neo4j

    def fake_build(config):
        built["uri"] = config.neo4j.uri
        return object()

    import cie.factory
    monkeypatch.setattr(cie.factory, "build_tool_service_from_config", fake_build)
    _fake_server_run(monkeypatch)
    (tmp_path / ".cie").mkdir()
    (tmp_path / ".cie" / "graph.db").write_text("")

    mcp_server.main([str(tmp_path), "--backend", "neo4j", "--neo4j-uri", "bolt://x:7687"])
    assert built["uri"] == "bolt://x:7687"
    assert "backend=neo4j" in capsys.readouterr().err


def test_mcp_env_selects_backend(tmp_path, monkeypatch, capsys):
    """CIE_BACKEND=embedded with no flags and no index — env is honored
    (same env the `cie` CLI already reads; one variable, both tools)."""
    import cie.mcp_server as mcp_server

    _fake_server_run(monkeypatch)
    monkeypatch.setenv("CIE_BACKEND", "embedded")
    mcp_server.main([str(tmp_path)])
    assert "backend=embedded" in capsys.readouterr().err


def test_mcp_embedded_alias_still_works(tmp_path, monkeypatch, capsys):
    """Every entry registered by `cie init` up to 0.1.2 carries
    `--embedded`; it must keep working forever (least friction = your
    existing registrations never break)."""
    import cie.mcp_server as mcp_server

    _fake_server_run(monkeypatch)
    monkeypatch.delenv("CIE_BACKEND", raising=False)
    # no graph.db: auto would pick neo4j — the alias forces embedded
    mcp_server.main([str(tmp_path), "--embedded"])
    assert "backend=embedded" in capsys.readouterr().err


def test_mcp_flag_beats_env(tmp_path, monkeypatch, capsys):
    import cie.mcp_server as mcp_server
    import cie.factory

    def fake_build(config):
        return object()

    monkeypatch.setattr(cie.factory, "build_tool_service_from_config", fake_build)
    _fake_server_run(monkeypatch)
    monkeypatch.setenv("CIE_BACKEND", "embedded")
    mcp_server.main([str(tmp_path), "--backend", "neo4j"])
    assert "backend=neo4j" in capsys.readouterr().err


def test_mcp_parser_accepts_backend_choices():
    from cie.mcp_server import _build_arg_parser

    parser = _build_arg_parser()
    args = parser.parse_args(["/proj", "--backend", "auto"])
    assert args.backend == "auto"
    with pytest.raises(SystemExit):
        parser.parse_args(["/proj", "--backend", "sqlite3"])