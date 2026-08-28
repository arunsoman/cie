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
    with pytest.raises(RuntimeError, match="task tracking was disabled"):
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
    # task_tracking=False so this test isolates the *graph* db_path override
    # — task tracking's own db_path is covered by
    # test_build_tool_service_embedded_custom_task_db_path below.
    build_tool_service_embedded(tmp_path, db_path=custom_db, task_tracking=False)
    assert custom_db.exists()
    assert not (tmp_path / ".cie").exists()


def test_build_tool_service_embedded_task_tracking_on_by_default(tmp_path):
    build_tool_service_embedded(tmp_path)
    assert (tmp_path / ".cie" / "graph.db").exists()
    assert (tmp_path / ".cie" / "tasks.db").exists()


def test_build_tool_service_embedded_custom_task_db_path(tmp_path):
    custom_task_db = tmp_path / "elsewhere" / "tasks.db"
    build_tool_service_embedded(tmp_path, task_db_path=custom_task_db)
    assert custom_task_db.exists()


def test_build_tool_service_embedded_task_tracking_false_uses_null_repo(tmp_path):
    service = build_tool_service_embedded(tmp_path, task_tracking=False)
    envelope = service.list_pending_tasks()
    assert envelope["ok"] is False
    assert "task tracking was disabled" in envelope["error"]["message"]
    assert not (tmp_path / ".cie" / "tasks.db").exists()


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


def test_embedded_toolservice_task_tools_work_by_default(tmp_path):
    """Since docs/growth-plan.md Phase 0.5 workstream B: task tracking is
    no longer a NullTaskRepository fail-fast case in the default embedded
    ToolService — see test_build_tool_service_embedded_task_tracking_false_uses_null_repo
    above for the explicit opt-out this test used to be the only path."""
    service = build_tool_service_embedded(tmp_path)
    envelope = service.list_pending_tasks()
    assert envelope["ok"] is True
    assert envelope["results"] == []


# --------------------------------------------------------------------------
# Ambiguous-name resolution fix — docs/competitor-benchmarks.md's Task 2
# (found 2026-08-28 while benchmarking against CodeGraphContext, fixed here)
# --------------------------------------------------------------------------

def _ambiguous_name_graph():
    """Two distinct classes (A, B) each define their own `helper` method,
    called once from within their own class and once more from an
    unrelated free function — deliberately reproducing the real
    `docs/competitor-benchmarks.md` scenario (two classes each with their
    own `_post` method) at a scale small enough to hand-verify."""
    nodes = [
        {"id": "f.py::A", "label": "A", "kind": "class", "source_file": "f.py"},
        {"id": "f.py::A.helper@2", "label": "helper", "kind": "method", "source_file": "f.py", "line_start": 2},
        {"id": "f.py::A.caller@3", "label": "caller", "kind": "method", "source_file": "f.py", "line_start": 3},
        {"id": "f.py::B", "label": "B", "kind": "class", "source_file": "f.py"},
        {"id": "f.py::B.helper@6", "label": "helper", "kind": "method", "source_file": "f.py", "line_start": 6},
        {"id": "f.py::B.caller@7", "label": "caller", "kind": "method", "source_file": "f.py", "line_start": 7},
        {"id": "f.py::B.other_caller@8", "label": "other_caller", "kind": "method", "source_file": "f.py", "line_start": 8},
    ]
    edges = [
        {"source": "f.py::A.caller@3", "target": "f.py::A.helper@2", "relation": "calls", "confidence": "EXTRACTED"},
        {"source": "f.py::B.caller@7", "target": "f.py::B.helper@6", "relation": "calls", "confidence": "EXTRACTED"},
        {"source": "f.py::B.other_caller@8", "target": "f.py::B.helper@6", "relation": "calls", "confidence": "EXTRACTED"},
    ]
    return nodes, edges


def test_resolve_symbol_ids_returns_every_exact_match(tmp_path):
    repo = EmbeddedRepository(tmp_path / "graph.db")
    nodes, edges = _ambiguous_name_graph()
    repo.load_extraction(nodes, edges)

    ids = repo._resolve_symbol_ids("helper")
    assert set(ids) == {"f.py::A.helper@2", "f.py::B.helper@6"}


def test_resolve_symbol_ids_falls_back_to_single_match_when_unambiguous(tmp_path):
    repo = EmbeddedRepository(tmp_path / "graph.db")
    nodes, edges = _ambiguous_name_graph()
    repo.load_extraction(nodes, edges)

    # "caller" is deliberately ambiguous too (both A and B define one) —
    # "other_caller" is the one genuinely unique name in this fixture.
    ids = repo._resolve_symbol_ids("other_caller")
    assert ids == ["f.py::B.other_caller@8"]


def test_get_callers_aggregates_across_every_ambiguous_definition(tmp_path):
    """The actual bug: get_callers("helper") used to see only ONE of the
    two `helper` methods and silently miss the other's callers entirely.
    Ground truth here is 3 real callers across both definitions."""
    repo = EmbeddedRepository(tmp_path / "graph.db")
    nodes, edges = _ambiguous_name_graph()
    repo.load_extraction(nodes, edges)

    callers = repo.get_callers("helper", limit=30)
    assert len(callers) == 3
    caller_ids = {er.edge.source for er in callers}
    assert caller_ids == {"f.py::A.caller@3", "f.py::B.caller@7", "f.py::B.other_caller@8"}


def test_get_callees_aggregates_across_every_ambiguous_definition(tmp_path):
    """Same bug, the other direction: two distinct 'caller' methods
    (in A and B) each call their own class's 'helper' — get_callees
    must see both, not just one class's outgoing call."""
    repo = EmbeddedRepository(tmp_path / "graph.db")
    nodes, edges = _ambiguous_name_graph()
    repo.load_extraction(nodes, edges)

    callees = repo.get_callees("caller", limit=30)
    targets = {er.edge.target for er in callees}
    assert targets == {"f.py::A.helper@2", "f.py::B.helper@6"}


def test_toolservice_callers_ambiguous_name_end_to_end(tmp_path):
    """Same fix, exercised through the real ToolService envelope (not the
    Repository directly) — this is what an MCP caller actually sees."""
    repo = EmbeddedRepository(tmp_path / ".cie" / "graph.db")
    nodes, edges = _ambiguous_name_graph()
    repo.load_extraction(nodes, edges)

    service = build_tool_service_embedded(tmp_path)
    envelope = service.callers("helper")
    assert envelope["ok"] is True
    assert envelope["total"] == 3
    assert envelope["truncated"] is False
