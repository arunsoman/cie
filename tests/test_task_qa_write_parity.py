"""R1 — task/QA write-side parity: the six promoted ToolService methods.

History this file pins: `push_tasks`/`set_task_status`/`link_artifact`/
`append_repair_events`/`record_coverage`/`record_coverage_snapshot` were
bespoke HTTP-only `_tool_*` handlers in `cie/routes.py` — invisible to
`ToolService.describe()`, to `ToolPolicy`, and to the MCP server — which
made the task/QA layer (the README's headline differentiator) read-only
on the default `cie-mcp --embedded` install. They are ToolService methods
now; these tests pin envelope + validation semantics on the embedded
backend (`tests/test_tool_surface_invariants.py` pins the structural
side, `tests/test_http_policy.py` the 403-by-default side).

Every assertion runs against `build_tool_service_embedded` — the same
backend a `cie-mcp --embedded` server serves — and the writes are
verified against the SQLite file on disk, not just the return envelope.
"""

from __future__ import annotations

import json
import sqlite3

import pytest

from cie.factory import build_tool_service_embedded
from cie.tool_policy import (
    FORGE_POLICY,
    INSPECTOR_POLICY,
    ToolNotPermitted,
    authorize,
)

VALID_TASK = {
    "name": "t1",
    "task_type": "dev",
    "description": "implement alpha's next step",
    "file_path": "app.py",
    "function_signatures": ["alpha()"],
    "test_triad": {"positive": "p", "negative": "n", "negative_to_positive": "n2p"},
}


@pytest.fixture()
def svc(tmp_path):
    (tmp_path / "app.py").write_text("def alpha():\n    return 1\n")
    # exercise the real indexing path so FILE nodes exist for coverage
    from cie.callgraph import resolve_call_edges, resolve_inheritance_edges
    from cie.extract import extract_many
    from cie.embedded_repository import EmbeddedRepository

    per_file = extract_many(tmp_path)
    nodes = [n for ext in per_file for n in ext.nodes]
    edges = (
        [e for ext in per_file for e in ext.edges]
        + resolve_call_edges(per_file)
        + resolve_inheritance_edges(per_file)
    )
    EmbeddedRepository(tmp_path / ".cie" / "graph.db").load_extraction(nodes, edges)
    return build_tool_service_embedded(tmp_path)


def _task_rows(tasks_db) -> list:
    """(name, status) pairs, parsed from the EmbeddedTaskRepository row
    shape (the `props` column holds the full task JSON)."""
    db = sqlite3.connect(tasks_db)
    try:
        return [
            (json.loads(props)["name"], json.loads(props)["status"])
            for (props,) in db.execute("select props from tasks")
        ]
    finally:
        db.close()


# ---------------------------------------------------------------------------
# push_tasks — partial accept + persistence + idempotence
# ---------------------------------------------------------------------------


def test_push_tasks_accepts_and_persists(svc, tmp_path):
    env = svc.push_tasks([VALID_TASK])
    assert env["ok"] is True
    assert env["results"]["accepted"] == 1
    assert env["results"]["rejected"] == []
    assert _task_rows(tmp_path / ".cie" / "tasks.db") == [("t1", "pending")]


def test_push_tasks_partially_accepts_invalid_batch(svc, tmp_path):
    bad = {**VALID_TASK, "name": "bad", "task_type": " nonsense"}
    good = {**VALID_TASK, "name": "good"}
    env = svc.push_tasks([good, bad])
    assert env["ok"] is True
    assert env["results"]["accepted"] == 1
    assert env["results"]["rejected"][0]["name"] == "bad"
    assert "re-push" in (env["hint"] or "")
    # the VALID half landed — never drop the whole batch silently
    assert _task_rows(tmp_path / ".cie" / "tasks.db") == [("good", "pending")]


def test_push_tasks_upserts_on_same_task_id(svc, tmp_path):
    """Idempotence is keyed on (task id, project) — plan_push rejects
    duplicate NAMES only within one batch; a re-push carrying the same
    explicit id upserts that row (matches the Neo4j MERGE-on-id
    semantics plan_push's own docstring documents — kept identical in
    the R1 promotion, never re-invented)."""
    task = {**VALID_TASK, "id": "fixed-id-1"}
    env = svc.push_tasks([task])
    assert env["results"]["accepted"] == 1
    env = svc.push_tasks([task])
    assert env["results"]["accepted"] == 1
    assert env["results"]["rejected"] == []
    assert _task_rows(tmp_path / ".cie" / "tasks.db") == [("t1", "pending")]


# ---------------------------------------------------------------------------
# set_task_status — lifecycle + validation + unknown
# ---------------------------------------------------------------------------


def test_set_task_status_lifecycle(svc, tmp_path):
    svc.push_tasks([VALID_TASK])
    env = svc.set_task_status("t1", "generated")
    assert env["ok"] is True
    assert env["results"]["status"] == "generated"
    assert env["results"]["updated_at"]
    assert _task_rows(tmp_path / ".cie" / "tasks.db") == [("t1", "generated")]


def test_set_task_status_invalid_is_a_validation_envelope(svc):
    env = svc.set_task_status("t1", "bogus")
    assert env["ok"] is False and env["error"]["kind"] == "validation"
    assert "'pending'," in env["hint"]  # the hint enumerates the real enum


def test_set_task_status_unknown_task_is_not_found(svc):
    env = svc.set_task_status("no-such-task", "pending")
    assert env["ok"] is False and env["error"]["kind"] == "not_found"
    assert "list_pending_tasks" in env["hint"]


# ---------------------------------------------------------------------------
# link_artifact — PRODUCED edge + validation + unknown
# ---------------------------------------------------------------------------


def test_link_artifact_roundtrip(svc):
    svc.push_tasks([VALID_TASK])
    env = svc.link_artifact("t1", "test", "test_app.py", "abc123")
    assert env["ok"] is True
    assert env["results"] == {"task": "t1", "path": "test_app.py", "kind": "test"}


def test_link_artifact_invalid_kind(svc):
    svc.push_tasks([VALID_TASK])
    env = svc.link_artifact("t1", "bogus", "x")
    assert env["ok"] is False and env["error"]["kind"] == "validation"
    assert "'source'," in env["hint"]


def test_link_artifact_unknown_task(svc):
    env = svc.link_artifact("nope", "source", "x")
    assert env["ok"] is False and env["error"]["kind"] == "not_found"


def test_link_artifact_kind_default_is_source(svc):
    svc.push_tasks([VALID_TASK])
    env = svc.link_artifact("t1", "", "x.py")
    assert env["results"]["kind"] == "source"


# ---------------------------------------------------------------------------
# append_repair_events — trajectory rows
# ---------------------------------------------------------------------------


EVENT = {"round": 1, "failures_before": 2, "failures_after": 1,
         "mode": "fix", "lint_ok": True, "timestamp": "2026-08-30T00:00:00Z"}


def test_append_repair_events_roundtrip(svc, tmp_path):
    svc.push_tasks([VALID_TASK])
    env = svc.append_repair_events("t1", [EVENT])
    assert env["ok"] is True
    assert env["results"] == {"task": "t1", "events_appended": 1}
    db = sqlite3.connect(tmp_path / ".cie" / "tasks.db")
    try:
        assert db.execute("select count(*) from events").fetchone()[0] >= 1
    finally:
        db.close()


def test_append_repair_events_invalid_shape_validates(svc):
    svc.push_tasks([VALID_TASK])
    env = svc.append_repair_events("t1", [{"round": "x"}])
    assert env["ok"] is False and env["error"]["kind"] == "validation"
    assert "RepairEvent shape" in env["hint"]


def test_append_repair_events_unknown_task(svc):
    env = svc.append_repair_events("nope", [EVENT])
    assert env["ok"] is False and env["error"]["kind"] == "not_found"


# ---------------------------------------------------------------------------
# record_coverage / record_coverage_snapshot — engine-backed QA writes
# ---------------------------------------------------------------------------


def test_record_coverage_roundtrip(svc):
    # paths are matched exactly against the indexed source_file, which
    # extract_many stamps as ABSOLUTE — take it from the index itself
    file_node = svc._engine.list_files()[0]  # noqa: SLF001 - test double-lite
    env = svc.record_coverage(
        file_node.source_file, coverage_pct=90.0, covered_lines=9,
        uncovered_lines=[10],
    )
    assert env["ok"] is True
    result = env["results"]
    assert result["coverage_pct"] == 90.0
    assert result["functions"], "per-function breakdown derived"


def test_record_coverage_unknown_file_is_not_found(svc):
    env = svc.record_coverage("never/indexed.py", coverage_pct=50.0)
    assert env["ok"] is False and env["error"]["kind"] == "not_found"
    assert "index the file first" in env["hint"]


def test_record_coverage_missing_file_path_is_validation(svc):
    env = svc.record_coverage("", coverage_pct=50.0)
    assert env["ok"] is False and env["error"]["kind"] == "validation"


def test_record_coverage_snapshot_and_trend(svc):
    snap = svc.record_coverage_snapshot(
        aggregate_pct=80.0, files_measured=1, total_lines=10, covered_lines=8,
        subtree="", measured_at="2026-08-30T00:00:00Z",
    )
    assert snap["ok"] is True
    assert snap["results"]["aggregate_pct"] == 80.0
    trend = svc._engine.coverage_trend("", 10)
    assert [s.aggregate_pct for s in trend] == [80.0]


# ---------------------------------------------------------------------------
# ToolPolicy gates the promoted writes on both sides
# ---------------------------------------------------------------------------

PROMOTED = (
    "push_tasks", "set_task_status", "link_artifact",
    "append_repair_events", "record_coverage", "record_coverage_snapshot",
)


def test_inspector_policy_refuses_every_promoted_write():
    for name in PROMOTED:
        try:
            authorize(INSPECTOR_POLICY, name)
        except ToolNotPermitted:
            pass
        else:
            raise AssertionError(
                f"INSPECTOR_POLICY must refuse {name} — the read-only "
                "story breaks the moment an MCP/HTTP caller can write"
            )


def test_forge_policy_permits_every_promoted_write():
    for name in PROMOTED:
        authorize(FORGE_POLICY, name)  # raises if refused