"""R14 — the PRD-hierarchy store over embedded SQLite.

Parity bar inherited from B1 (tests/test_embedded_task_repository.py's
17-test depth): every protocol method, every validation rule, project
scoping, on-disk persistence, and the ToolService promotion roundtrips
(`push_hierarchy`/`get_children`/`get_lineage` were the LAST HTTP-only
alias handlers before R14 promoted them).

The on-disk store under test: `cie/embedded_hierarchy_repository.py`
(same `HierarchyRepository` protocol as `Neo4jHierarchyRepository`;
intentional backend differences — single HAS_CHILD direction,
name-keyed REALIZED_BY — are documented in that module's docstring and
asserted here).
"""

from __future__ import annotations

import json
import sqlite3

import pytest

from cie.embedded_hierarchy_repository import SQLiteHierarchyRepository
from cie.factory import build_tool_service_embedded
from cie.tasks import HierarchyNode
from cie.tool_policy import FORGE_POLICY, INSPECTOR_POLICY, authorize

R = HierarchyNode


def tree() -> HierarchyNode:
    return R(node_type="module", id="m1", name="Backend", children=[
        R(node_type="feature", id="f1", name="Auth", children=[
            R(node_type="userstory", id="u1", name="Login",
              metadata={"task_names": ["t1"]}),
            R(node_type="userstory", id="u2", name="Logout"),
        ]),
    ])


@pytest.fixture()
def repo(tmp_path):
    return SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")


def _named_rows(tmp_path):
    db = sqlite3.connect(tmp_path / ".cie" / "hierarchy.db")
    try:
        nodes = {
            (name, ntype) for name, ntype in db.execute(
                "select name, node_type from hierarchy_nodes"
            )
        }
        has_child = db.execute(
            "select count(*) from hierarchy_edges where kind = 'HAS_CHILD'"
        ).fetchone()[0]
        realized = db.execute(
            "select count(*) from hierarchy_edges where kind = 'REALIZED_BY'"
        ).fetchone()[0]
    finally:
        db.close()
    return nodes, has_child, realized


# ---------------------------------------------------------------------------
# push_hierarchy — MERGE semantics, validation, idempotence
# ---------------------------------------------------------------------------


def test_push_writes_the_whole_tree(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    assert repo.push_hierarchy(tree()) == 4
    nodes, has_child, realized = _named_rows(tmp_path)
    assert {n for n, _ in nodes} == {"Backend", "Auth", "Login", "Logout"}
    assert has_child == 3
    # REALIZED_BY is name-keyed and unconditional on this backend (t1
    # needn't exist in a separate tasks.db — documented difference)
    assert realized == 1


def test_push_is_idempotent_by_node_id(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(tree())
    # re-push the same id but a changed name — MERGE updates in place,
    # never forks a second node (the (id, project) key)
    repo.push_hierarchy(R(node_type="module", id="m1", name="Backend v2"))
    assert repo.get_hierarchy_node("m1").name == "Backend v2"
    # children not in the new tree are NOT deleted (MERGE, not replace):
    # the Neo4j backend has identical semantics ("MERGE the whole tree")
    assert repo.get_hierarchy_node("u2").name == "Logout"


def test_push_rejects_repeated_id_on_a_root_to_leaf_path(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    bad = R(node_type="module", id="m1", name="X", children=[
        R(node_type="feature", id="m1", name="Y"),
    ])
    with pytest.raises(ValueError, match="repeats id 'm1' on a root-to-leaf path"):
        repo.push_hierarchy(bad)


def test_push_empty_tree(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    assert repo.push_hierarchy(R(node_type="module", id="solo", name="Only")) == 1


# ---------------------------------------------------------------------------
# get_children — BFS order, depth, type filter, truncation
# ---------------------------------------------------------------------------


def _wider_tree():
    return R(node_type="module", id="m1", name="Backend", children=[
        R(node_type="feature", id="f2", name="Auth", children=[
            R(node_type="userstory", id="u1", name="Login"),
            R(node_type="workflow", id="w1", name="Passes-through"),
        ]),
        R(node_type="feature", id="f3", name="Billing"),
    ])


def test_get_children_bfs_ordered_by_depth_then_name(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(_wider_tree())
    subtree = repo.get_children("m1")
    assert [(c.id, c.depth) for c in subtree.children] == [
        ("f2", 1), ("f3", 1), ("u1", 2), ("w1", 2),
    ]


def test_get_children_depth_one(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(_wider_tree())
    subtree = repo.get_children("m1", depth=1)
    assert [c.id for c in subtree.children] == ["f2", "f3"]


def test_get_children_traversal_passes_through_filtered_types(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(_wider_tree())
    subtree = repo.get_children("m1", type_filter="userstory")
    assert [c.id for c in subtree.children] == ["u1"]


def test_get_children_limit_marks_truncated(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(_wider_tree())
    subtree = repo.get_children("m1", limit=2)
    assert subtree.truncated is True and len(subtree.children) == 2


def test_get_children_unknown_node_raises_valueerror(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    with pytest.raises(ValueError, match="unknown hierarchy node 'nope'"):
        repo.get_children("nope")


def test_get_children_metadata_is_lossless(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(_wider_tree())
    m = _wider_tree()
    repo.push_hierarchy(R(node_type="userstory", id="meta1", name="M",
                          metadata={"task_names": ["t9"], "priority": 5}))
    view = repo.get_hierarchy_node("meta1")
    assert view.metadata == {"task_names": ["t9"], "priority": 5}


# ---------------------------------------------------------------------------
# get_lineage / get_hierarchy_node / get_project_tree
# ---------------------------------------------------------------------------


def test_get_lineage_is_root_first_node_last(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(_wider_tree())
    lineage = repo.get_lineage("u1")
    assert [(v.id, v.depth) for v in lineage] == [("m1", 0), ("f2", 1), ("u1", 2)]


def test_get_lineage_unknown_returns_empty(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    assert repo.get_lineage("nope") == []


def test_get_hierarchy_node_none_for_unknown(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    assert repo.get_hierarchy_node("nope") is None
    repo.push_hierarchy(_wider_tree())
    view = repo.get_hierarchy_node("f3")
    assert (view.node_type, view.name) == ("feature", "Billing")


def test_get_project_tree_resolves_parent_and_children(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(_wider_tree())
    views = {v.id: v for v in repo.get_project_tree("")}
    assert views["f3"].parent_id == "m1"
    assert views["m1"].children_ids == ["f2", "f3"]
    assert views["m1"].parent_id is None


def test_project_scoping_isolates_projects(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db", project="")
    repo.push_hierarchy(tree(), project="")
    repo.push_hierarchy(R(node_type="module", id="m_other", name="Other"), project="p2")
    assert {v.id for v in repo.get_project_tree("")} == {"m1", "f1", "u1", "u2"}
    assert {v.id for v in repo.get_project_tree("p2")} == {"m_other"}
    # cross-project reads of the same id never leak (the (id, project) key)
    repo.push_hierarchy(R(node_type="module", id="m1", name="Other project's m1"),
                        project="p2")
    assert repo.get_hierarchy_node("m1") is not None  # default still has its own
    p2 = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db", project="p2")
    assert p2.get_hierarchy_node("m1").name == "Other project's m1"


def test_hierarchy_db_is_inspectable_sqlite(tmp_path):
    repo = SQLiteHierarchyRepository(tmp_path / ".cie" / "hierarchy.db")
    repo.push_hierarchy(tree())
    db = sqlite3.connect(tmp_path / ".cie" / "hierarchy.db")
    try:
        raw = json.loads(db.execute(
            "select metadata_json from hierarchy_nodes where id = 'u1'"
        ).fetchone()[0])
        assert raw == {"task_names": ["t1"]}
    finally:
        db.close()


# ---------------------------------------------------------------------------
# ToolService promotion roundtrips (the R14 half of R1's parity contract)
# ---------------------------------------------------------------------------


@pytest.fixture()
def svc(tmp_path):
    (tmp_path / "app.py").write_text("def alpha():\n    return 1\n")
    return build_tool_service_embedded(tmp_path)


def _tree_dict():
    return {
        "node_type": "module", "id": "m1", "name": "Backend",
        "children": [
            {"node_type": "feature", "id": "f1", "name": "Auth",
             "children": [
                 {"node_type": "userstory", "id": "u1", "name": "Login"},
             ]},
        ],
    }


def test_toolservice_push_children_lineage_roundtrip_on_embedded(svc, tmp_path):
    env = svc.push_hierarchy(tree=_tree_dict())
    assert env["ok"] is True and env["results"]["nodes_written"] == 3
    assert (tmp_path / ".cie" / "hierarchy.db").is_file()
    env = svc.get_children(node_id="m1")
    assert env["ok"] is True
    assert [c["id"] for c in env["results"]["children"]] == ["f1", "u1"]
    env = svc.get_lineage(node_id="u1")
    assert [v["id"] for v in env["results"]] == ["m1", "f1", "u1"]


def test_toolservice_get_children_unknown_is_not_found(svc):
    env = svc.get_children(node_id="nope")
    assert env["ok"] is False and env["error"]["kind"] == "not_found"


def test_toolservice_push_hierarchy_invalid_tree_is_validation(svc):
    env = svc.push_hierarchy(tree={"node_type": "not_a_type"})
    assert env["ok"] is False and env["error"]["kind"] == "validation"


def test_toolservice_hierarchy_tracking_false_is_unavailable_with_reason(tmp_path):
    svc = build_tool_service_embedded(tmp_path, hierarchy_tracking=False)
    env = svc.get_lineage(node_id="x")
    assert env["ok"] is False and env["error"]["kind"] == "unavailable"
    assert env["error"]["reason"] == "HIERARCHY_STORE_NOT_CONFIGURED"


def test_push_hierarchy_policy_write_read_split():
    authorize(INSPECTOR_POLICY, "get_children")
    authorize(INSPECTOR_POLICY, "get_lineage")
    try:
        authorize(INSPECTOR_POLICY, "push_hierarchy")
    except Exception as exc:  # noqa: BLE001
        from cie.tool_policy import ToolNotPermitted

        assert isinstance(exc, ToolNotPermitted)
    else:
        raise AssertionError("INSPECTOR must refuse push_hierarchy")
    authorize(FORGE_POLICY, "push_hierarchy")