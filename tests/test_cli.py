"""R2 — CLI↔embedded-SQLite parity: the zero-config quickstart contract.

The regression this file exists to prevent (session-7 log, named not
fixed): `cie index` wrote the local SQLite graph while every documented
query command hardwired `Neo4jRepository.connect` — so the quickstart
literally retried `localhost:7687` four times before failing. These
tests drive the REAL click tree (CliRunner, no monkeypatching of the
openers) through the exact quickstart sequence — `cie index .` then the
documented query commands — and assert they answer from `.cie/graph.db`
with zero bolt traffic, plus the honest-error edges added in the same
pass (explicit-embedded with a missing db, hierarchy on embedded,
--backend precedence).

Neo4j behaviors are NOT exercised here (no live server in CI); the
Neo4j construction branch is unchanged code asserted indirectly by the
backend-selection unit tests below.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from click.testing import CliRunner

from cie.cli import _embedded_db_path, _selected_backend, cli

APP_PY = (
    "def helper():\n"
    "    return 0\n"
    "\n"
    "def beta():\n"
    "    return helper()\n"
    "\n"
    "def alpha():\n"
    "    return beta()\n"
)

CLASS_APP_PY = (
    "class Greeter:\n"
    "    def greet(self, name: str) -> str:\n"
    "        return name\n"
    "\n"
    "def alpha():\n"
    "    return Greeter().greet(\"hello\")\n"
)

TASK_BATCH = {
    "tasks": [
        {
            "name": "t1",
            "task_type": "dev",
            "description": "implement alpha's next step",
            "file_path": "app.py",
            "function_signatures": ["alpha()"],
            # a dev task must carry its test triad (push_tasks' own
            # validation — the repo-level QA contract, unchanged by R2)
            "test_triad": {
                "positive": "test_alpha_happy",
                "negative": "test_alpha_negative",
                "negative_to_positive": "test_alpha_after_fix",
            },
        }
    ]
}

#: CIE_* env vars that could leak in from a dev shell and flip the
#: selection rule — pinned to absent for every test here.
ENV_TO_CLEAR = (
    "CIE_PROJECT", "FORGE_PROJECT_ID", "CIE_BACKEND", "CIE_DB",
    "CIE_NEO4J_URI", "CIE_NEO4J_USER", "CIE_NEO4J_PASSWORD", "CIE_RUN_ROOT",
    "CIE_NO_TASK_TRACKING",
)


@pytest.fixture()
def runner(monkeypatch):
    for name in ENV_TO_CLEAR:
        monkeypatch.delenv(name, raising=False)
    return CliRunner()


def _invoke_json(runner, args, **kwargs):
    """Run a CLI command in --json mode; returns the parsed SPEC §0 envelope."""
    result = runner.invoke(cli, ["--json", *args], catch_exceptions=False, **kwargs)
    assert result.exit_code == kwargs.pop("exit_code", 0), result.output
    return json.loads(result.output)


@pytest.fixture()
def indexed_project(runner):
    """The quickstart itself: a 1-file project, `cie index .` — nothing else."""
    with runner.isolated_filesystem():
        Path("app.py").write_text(APP_PY)
        env = json.loads(
            runner.invoke(cli, ["--json", "index", "."]).output
        )
        assert env["ok"] is True
        yield Path.cwd()


@pytest.fixture()
def indexed_class_project(runner):
    """An embedded graph with a class for signature and method queries."""
    with runner.isolated_filesystem():
        Path("app.py").write_text(CLASS_APP_PY)
        env = json.loads(
            runner.invoke(cli, ["--json", "index", "."]).output
        )
        assert env["ok"] is True
        yield Path.cwd()


# ---------------------------------------------------------------------------
# The quickstart contract: index writes it, every query command reads it
# ---------------------------------------------------------------------------


class TestQuickstartParity:
    def test_engine_backed_commands_answer_on_embedded(
        self, runner, indexed_project
    ):
        for args in (
            ["files"], ["stats"], ["health"], ["discover"], ["node", "alpha"],
            ["neighbors", "alpha"], ["search", "alpha"],
        ):
            env = json.loads(runner.invoke(cli, ["--json", *args]).output)
            assert env["ok"] is True, (args, env)

    def test_tool_service_backed_commands_answer_on_embedded(
        self, runner, indexed_project
    ):
        # callers: alpha is a leaf — the ground truth is who calls *beta*
        env = json.loads(
            runner.invoke(cli, ["--json", "callers", "beta"]).output
        )
        assert env["ok"] is True
        assert {r["caller"] for r in env["results"]} == {"alpha"}
        env = json.loads(
            runner.invoke(cli, ["--json", "callees", "beta"]).output
        )
        assert {r["callee"] for r in env["results"]} == {"helper"}
        # skeleton + view-file + search-symbol
        env = json.loads(
            runner.invoke(cli, ["--json", "skeleton", "app.py"]).output
        )
        assert {s["name"] for s in env["results"][0]["symbols"]} == {
            "helper", "beta", "alpha",
        }
        env = json.loads(
            runner.invoke(cli, ["--json", "search-symbol", "alpha"]).output
        )
        assert env["results"][0]["source_file"].endswith("app.py")
        env = json.loads(
            runner.invoke(cli, ["--json", "view-file", "app.py"]).output
        )
        assert "def alpha():" in env["results"][0]["content"]
        env = json.loads(
            runner.invoke(cli, ["--json", "path", "alpha", "helper"]).output
        )
        assert env["ok"] is True

    def test_remaining_query_commands_answer_on_embedded(
        self, runner, indexed_class_project
    ):
        env = _invoke_json(runner, ["signature", "alpha"])
        assert env["results"]["signature"] == "alpha()"

        env = _invoke_json(runner, ["methods", "Greeter"])
        assert [method["signature"] for method in env["results"]] == [
            "greet(self, name: str) -> str"
        ]

        # The embedded index does not calculate communities, but these CLI
        # commands must still return their documented empty envelopes.
        env = _invoke_json(runner, ["communities"])
        assert env["results"] == []
        env = _invoke_json(runner, ["community", "0"])
        assert env["results"] == []

        # Coverage read commands also answer against an indexed embedded graph
        # before any external coverage producer has recorded measurements.
        env = _invoke_json(runner, ["coverage:report"])
        assert [(row["file_path"], row["measured"])
                for row in env["results"]] == [
            (str(indexed_class_project / "app.py"), False)
        ]
        env = _invoke_json(runner, ["coverage:trend"])
        assert env["results"] == []

    def test_task_roundtrip_lands_in_sibling_tasks_db(
        self, runner, indexed_project
    ):
        Path("batch.json").write_text(json.dumps(TASK_BATCH))
        env = json.loads(
            runner.invoke(cli, ["--json", "tasks:push", "batch.json"]).output
        )
        assert env["ok"] is True and env["results"]["accepted"] == 1
        assert (Path.cwd() / ".cie" / "tasks.db").is_file()
        env = json.loads(
            runner.invoke(cli, ["--json", "tasks:pending"]).output
        )
        assert [t["name"] for t in env["results"]] == ["t1"]
        env = json.loads(
            runner.invoke(cli, ["--json", "tasks:get", "t1"]).output
        )
        assert env["results"][0]["file_path"] == "app.py"
        env = _invoke_json(runner, ["tasks:closure", "t1"])
        assert env["results"] == []
        env = _invoke_json(runner, ["tasks:deps", "t1"])
        assert env["results"] == []
        env = _invoke_json(runner, ["validate:coverage"])
        assert [gap["task_name"] for gap in env["results"]] == ["t1"]
        env = json.loads(
            runner.invoke(cli, ["--json", "validate:cycles"]).output
        )
        assert env["results"]["has_cycle"] is False

    def test_human_mode_renders_without_traceback(self, runner, indexed_project):
        result = runner.invoke(cli, ["search-symbol", "alpha"])
        assert result.exit_code == 0
        assert "alpha" in result.output
        assert "Traceback" not in result.output


# ---------------------------------------------------------------------------
# The selection rule itself
# ---------------------------------------------------------------------------


class TestBackendSelection:
    def _ctx(self, backend=None, db=None):
        """Deadline-simple stand-in for the click group context."""
        import types

        ctx = types.SimpleNamespace()
        ctx.obj = {"backend_opt": backend, "db_opt": db}
        return ctx

    def test_explicit_backend_beats_everything(self, tmp_path, monkeypatch):
        monkeypatch.setenv("CIE_BACKEND", "neo4j")
        db = tmp_path / ".cie" / "graph.db"
        db.parent.mkdir(parents=True)
        db.write_text("")
        assert _selected_backend(self._ctx(db=db)) == "neo4j"
        monkeypatch.setenv("CIE_BACKEND", "embedded")
        assert _selected_backend(self._ctx(backend="neo4j")) == "neo4j"
        assert _selected_backend(self._ctx(backend="embedded")) == "embedded"

    def test_env_backend_is_honored(self, tmp_path, monkeypatch):
        monkeypatch.setenv("CIE_BACKEND", "embedded")
        assert _selected_backend(self._ctx()) == "embedded"
        monkeypatch.setenv("CIE_BACKEND", "neo4j")
        assert _selected_backend(self._ctx()) == "neo4j"

    def test_auto_prefers_existing_embedded_db(self, tmp_path, monkeypatch):
        monkeypatch.delenv("CIE_BACKEND", raising=False)
        monkeypatch.chdir(tmp_path)  # a cwd guaranteed db-free; the repo's own .cie would lie here
        assert _selected_backend(self._ctx()) == "neo4j"
        (tmp_path / ".cie").mkdir()
        (tmp_path / ".cie" / "graph.db").write_text("")
        assert _selected_backend(self._ctx()) == "embedded"

    def test_explicit_db_flag_shifts_the_probe(self, tmp_path):
        other = tmp_path / "elsewhere" / "graph.db"
        other.parent.mkdir(parents=True)
        other.write_text("")
        assert _selected_backend(self._ctx(db=other)) == "embedded"

    def test_explicit_auto_means_auto_not_neo4j(self, tmp_path, monkeypatch):
        """Regression (found live 2026-08-31): the pre-centralization
        `if opt: return opt` returned the truthy string "auto", and
        `_open_backend` then treated it as Neo4j — an indexed project
        queried the wrong store. Explicit --backend auto must MEAN auto."""
        monkeypatch.delenv("CIE_BACKEND", raising=False)
        monkeypatch.chdir(tmp_path)
        (tmp_path / ".cie").mkdir()
        (tmp_path / ".cie" / "graph.db").write_text("")
        assert _selected_backend(self._ctx(backend="auto")) == "embedded"
        (tmp_path / ".cie" / "graph.db").unlink()
        assert _selected_backend(self._ctx(backend="auto")) == "neo4j"

    def test_embedded_db_path_default_is_cwd(self, tmp_path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        assert _embedded_db_path(None) == tmp_path / ".cie" / "graph.db"
        assert _embedded_db_path(tmp_path / "x" / "g.db") == tmp_path / "x" / "g.db"


# ---------------------------------------------------------------------------
# The honest-error edges added with the seam
# ---------------------------------------------------------------------------


class TestHonestErrors:
    def test_explicit_embedded_with_missing_db_fails_fast(
        self, runner, tmp_path, monkeypatch
    ):
        """Not a traceback, not four bolt retries — one not_found envelope."""
        monkeypatch.chdir(tmp_path)  # no .cie/graph.db here
        result = runner.invoke(
            cli, ["--json", "--backend", "embedded", "stats"]
        )
        assert result.exit_code == 1
        env = json.loads(result.output)
        assert env["ok"] is False and env["error"]["kind"] == "not_found"
        assert "cie index" in env["hint"]

    def test_hierarchy_commands_work_on_embedded(self, runner, indexed_project):
        """R14 roundtrip through the real CLI: push -> children -> lineage,
        all against .cie/hierarchy.db (was honest-unavailable in R2, then
        R14 shipped the SQLite store — this test flipped from a negative
        pin to a positive one; the honest-unavailable contract it used to
        pin moved to `hierarchy_tracking=False` configurations)."""
        tree = {
            "node_type": "module", "id": "m1", "name": "Backend",
            "children": [
                {"node_type": "feature", "id": "f1", "name": "Auth",
                 "children": [
                     {"node_type": "userstory", "id": "u1", "name": "Login"},
                 ]},
            ],
        }
        Path("tree.json").write_text(json.dumps(tree))
        env = json.loads(
            runner.invoke(cli, ["--json", "hierarchy:push", "tree.json"]).output
        )
        assert env["ok"] is True and env["results"]["nodes_written"] == 3
        assert (Path.cwd() / ".cie" / "hierarchy.db").is_file()
        env = json.loads(
            runner.invoke(cli, ["--json", "hierarchy:children", "m1"]).output
        )
        assert env["ok"] is True
        assert [c["id"] for c in env["results"]["children"]] == ["f1", "u1"]
        env = json.loads(
            runner.invoke(cli, ["--json", "hierarchy:lineage", "u1"]).output
        )
        assert env["ok"] is True
        assert [v["id"] for v in env["results"]] == ["m1", "f1", "u1"]

    def test_hierarchy_commands_are_honest_unavailable_when_tracking_off(
        self, runner, tmp_path, monkeypatch
    ):
        """The honest-unavailable contract didn't die with R14 — it moved
        to the explicit opt-out: an embedded service built with the
        hierarchy store disabled returns the unavailable envelope (never
        silently-empty), via factory's hierarchy_tracking=False."""
        from cie.factory import build_tool_service_embedded

        svc = build_tool_service_embedded(tmp_path, hierarchy_tracking=False)
        env = svc.get_lineage("m1")
        assert env["ok"] is False and env["error"]["kind"] == "unavailable"
        assert env["error"]["reason"] == "HIERARCHY_STORE_NOT_CONFIGURED"

    def test_load_refuses_explicit_embedded_with_the_embedded_hint(
        self, runner, tmp_path, monkeypatch
    ):
        monkeypatch.chdir(tmp_path)
        result = runner.invoke(
            cli, ["--json", "--backend", "embedded", "load", "."]
        )
        assert result.exit_code == 1
        env = json.loads(result.output)
        assert env["error"]["kind"] == "unavailable"
        assert "cie index" in env["hint"]

    def test_bootstrap_refuses_explicit_embedded(self, runner, tmp_path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        result = runner.invoke(
            cli, ["--json", "--backend", "embedded", "bootstrap"]
        )
        assert result.exit_code == 1
        env = json.loads(result.output)
        assert env["error"]["kind"] == "unavailable"