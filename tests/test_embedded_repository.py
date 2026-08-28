"""Tests for cie.embedded_repository — the zero-config, no-Neo4j-required
graph backend (Phase 0 of docs/growth-plan.md).
"""

from __future__ import annotations

import sqlite3
from pathlib import Path

import pytest

from cie.embedded_repository import EmbeddedRepository, NullTaskRepository, load_graph, save_graph
from cie.factory import build_tool_service_embedded
from cie.models import NodeKind
from cie.tools import ToolService

_NODE_MAIN = {
    "id": "f.py::main@1", "label": "main", "kind": NodeKind.FUNC.value,
    "source_file": "f.py", "line_start": 1, "line_end": 3,
}
_NODE_HELPER = {
    "id": "f.py::helper@5", "label": "helper", "kind": NodeKind.FUNC.value,
    "source_file": "f.py", "line_start": 5, "line_end": 7,
}
_EDGE_CALLS = {
    "source": "f.py::main@1", "target": "f.py::helper@5",
    "relation": "calls", "confidence": "EXTRACTED",
}


def test_fresh_db_path_is_empty_not_an_error(tmp_path):
    repo = EmbeddedRepository(tmp_path / "graph.db")
    assert repo.stats().nodes == 0


def test_load_then_reopen_persists_across_a_simulated_restart(tmp_path):
    db = tmp_path / "graph.db"
    repo = EmbeddedRepository(db)
    written = repo.load_extraction([_NODE_MAIN, _NODE_HELPER], [_EDGE_CALLS])
    assert written == 2

    reopened = EmbeddedRepository(db)
    assert reopened.stats().nodes == 2
    assert reopened.stats().edges == 1
    matches = reopened.search_symbols("main")
    assert [m.node.label for m in matches] == ["main"]


def test_persisted_file_is_real_inspectable_sqlite(tmp_path):
    db = tmp_path / "graph.db"
    repo = EmbeddedRepository(db)
    repo.load_extraction([_NODE_MAIN], [])
    conn = sqlite3.connect(str(db))
    try:
        rows = conn.execute("SELECT id FROM nodes").fetchall()
    finally:
        conn.close()
    assert rows == [("f.py::main@1",)]


def test_callers_resolve_correctly_after_persistence_round_trip(tmp_path):
    db = tmp_path / "graph.db"
    EmbeddedRepository(db).load_extraction([_NODE_MAIN, _NODE_HELPER], [_EDGE_CALLS])
    reopened = EmbeddedRepository(db)
    callers = reopened.get_callers("helper")
    assert len(callers) == 1
    assert callers[0].source_label == "main"


def test_delete_then_reopen_reflects_the_deletion(tmp_path):
    db = tmp_path / "graph.db"
    repo = EmbeddedRepository(db)
    repo.load_extraction([_NODE_MAIN, _NODE_HELPER], [_EDGE_CALLS])
    repo.delete_nodes(["f.py::main@1"])

    reopened = EmbeddedRepository(db)
    assert reopened.stats().nodes == 1
    assert reopened.get_node("main") is None


def test_load_graph_helper_matches_save_graph_round_trip(tmp_path):
    db = tmp_path / "graph.db"
    repo = EmbeddedRepository(db)
    repo.load_extraction([_NODE_MAIN], [])
    nodes, edges = load_graph(db)
    assert len(nodes) == 1
    assert nodes[0].id == "f.py::main@1"
    assert edges == []


def test_null_task_repository_fails_fast_with_a_clear_message():
    task_repo = NullTaskRepository()
    with pytest.raises(RuntimeError, match="task tracking is not part of the zero-config"):
        task_repo.list_pending()
    with pytest.raises(RuntimeError, match="get_task"):
        task_repo.get_task("t-1")


def test_null_task_repository_private_attrs_raise_attribute_error_not_runtime_error():
    task_repo = NullTaskRepository()
    with pytest.raises(AttributeError):
        task_repo._internal_thing


# --------------------------------------------------------------------------
# build_tool_service_embedded — the full ToolService wiring
# --------------------------------------------------------------------------

def test_build_tool_service_embedded_needs_no_neo4j_no_env_vars(tmp_path):
    service = build_tool_service_embedded(tmp_path)
    assert isinstance(service, ToolService)
    assert (tmp_path / ".cie" / "graph.db").exists()  # created on first touch


def test_build_tool_service_embedded_custom_db_path(tmp_path):
    custom_db = tmp_path / "somewhere" / "custom.db"
    build_tool_service_embedded(tmp_path, db_path=custom_db)
    assert custom_db.exists()
    assert not (tmp_path / ".cie").exists()


def test_embedded_toolservice_search_symbol_and_callers_work_end_to_end(tmp_path):
    (tmp_path / "app.py").write_text(
        "def helper(x):\n    return x * 2\n\n"
        "def main():\n    y = helper(5)\n    print(y)\n    return y\n",
    )
    from cie import extract
    from cie.embedded_repository import EmbeddedRepository

    extraction = extract.extract_tree(tmp_path)
    EmbeddedRepository(tmp_path / ".cie" / "graph.db").load_extraction(
        extraction.nodes, extraction.edges,
    )

    service = build_tool_service_embedded(tmp_path)
    envelope = service.search_symbol("main")
    assert envelope["ok"] is True
    assert envelope["results"][0]["name"] == "main"

    callers = service.callers("helper")
    assert callers["ok"] is True
    assert callers["total"] == 1


def test_embedded_toolservice_task_tools_fail_fast_not_silently(tmp_path):
    service = build_tool_service_embedded(tmp_path)
    envelope = service.list_pending_tasks()
    assert envelope["ok"] is False
    assert "task tracking is not part of the zero-config" in envelope["error"]["message"]
