"""Tests for cie.embedded_task_repository — the zero-config, no-Neo4j
task/QA repository.

Every test exercises the SQLite-backed implementation directly (no mocks
for the storage layer) so a real bug in the SQL/serialization round trip
would actually fail here, the same standard test_embedded_repository.py
already holds the code-graph backend to.
"""

from __future__ import annotations

import pytest

from cie.embedded_task_repository import EmbeddedTaskRepository
from cie.task_repository import TaskRepository
from cie.tasks import (
    ApiSpec,
    Artifact,
    ArtifactKind,
    AtomicQaTask,
    AtomicTask,
    AtomicTaskBatch,
    RepairEvent,
    TestFramework,
    TestOrigin,
    TestTriad,
    TestType,
    TaskStatus,
)


def _dev_task(name: str, file_path: str, deps: list[str] | None = None, **kw) -> AtomicTask:
    return AtomicTask(
        name=name,
        task_type="dev",
        file_path=file_path,
        function_signatures=["def f(): ..."],
        test_triad=TestTriad(positive="p", negative="n", negative_to_positive="ntp"),
        dependencies=deps or [],
        **kw,
    )


def _qa_task(name: str, file_path: str, **kw) -> AtomicTask:
    return AtomicQaTask(
        name=name,
        task_type="qa",
        file_path=file_path,
        function_signatures=["def test_f(): ..."],
        test_framework=TestFramework.PYTEST,
        run_command="pytest tests/test_f.py",
        test_type=TestType.UNIT,
        test_origin=TestOrigin.GENERATED_BY_LLM,
        **kw,
    )


def test_satisfies_the_task_repository_protocol(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    assert isinstance(repo, TaskRepository)


def test_push_tasks_accepts_a_valid_task_and_it_is_pending(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    result = repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T1", "a.py")]))
    assert result.accepted == 1
    assert result.rejected == []
    pending = repo.list_pending()
    assert [t.name for t in pending] == ["T1"]


def test_push_tasks_rejects_a_dev_task_missing_test_triad_partial_accept(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    good = _dev_task("T1", "a.py")
    bad = AtomicTask(name="T2", task_type="dev", file_path="b.py", function_signatures=["def g(): ..."])
    result = repo.push_tasks(AtomicTaskBatch(tasks=[good, bad]))
    assert result.accepted == 1
    assert len(result.rejected) == 1
    assert "test_triad" in result.rejected[0].reason
    assert [t.name for t in repo.list_pending()] == ["T1"]


def test_get_task_round_trips_every_field(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    t = _dev_task("T1", "a.py", description="does a thing", exact_imports=["import os"])
    repo.push_tasks(AtomicTaskBatch(tasks=[t]))
    back = repo.get_task("T1")
    assert back is not None
    assert back.description == "does a thing"
    assert back.exact_imports == ["import os"]
    assert back.test_triad.positive == "p"


def test_set_status_removes_task_from_pending(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T1", "a.py")]))
    assert repo.set_status("T1", TaskStatus.TESTED_GREEN, "2026-08-29T00:00:00")
    assert repo.list_pending() == []
    assert repo.set_status("does-not-exist", TaskStatus.TESTED_GREEN, "x") is False


def test_set_qa_status_and_get_task_unaffected_by_it(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    repo.push_tasks(AtomicTaskBatch(tasks=[_qa_task("QA1", "a.py")]))
    assert repo.set_qa_status("QA1", "qa_generated", "2026-08-29T00:00:00")
    assert repo.get_task("QA1") is not None


def test_dependency_traversal_is_the_real_traceability_query(tmp_path):
    """The one query docs/competitive-landscape.md claims no competitor
    can answer: 'which files implement this task, and are they tested?'
    Modeled here as A depends on B's file, B depends on C's file."""
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    a = _dev_task("A", "a.py", deps=["b.py"])
    b = _dev_task("B", "b.py", deps=["c.py"])
    c = _dev_task("C", "c.py")
    repo.push_tasks(AtomicTaskBatch(tasks=[c]))  # write C first so B's dep resolves
    repo.push_tasks(AtomicTaskBatch(tasks=[b]))
    repo.push_tasks(AtomicTaskBatch(tasks=[a]))

    assert repo.get_dependencies("A") == ["b.py", "c.py"]
    dependents = {t.name for t in repo.get_dependent_tasks("A")}
    assert dependents == {"B", "C"}
    # Leaf task has no dependencies of its own.
    assert repo.get_dependencies("C") == []
    assert repo.get_dependent_tasks("C") == []


def test_validate_cycles_detects_a_cycle_bypassing_push_time_prevention(tmp_path):
    """push_tasks already rejects cycles at write time (plan_push, shared
    with Neo4jTaskRepository) — this test writes directly via the
    internal _write_tasks to simulate data that predates that check, the
    same way the validator would need to catch a legacy/corrupted state."""
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    a = _dev_task("A", "a.py", deps=["b.py"])
    b = _dev_task("B", "b.py", deps=["a.py"])
    repo._write_tasks([a, b])
    result = repo.validate_cycles()
    assert result.has_cycle is True
    assert set(result.cycle) == {"A", "B"}


def test_validate_cycles_clean_graph_has_none(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("A", "a.py")]))
    result = repo.validate_cycles()
    assert result.has_cycle is False
    assert result.cycle == []


def test_validate_coverage_flags_untested_dev_task_then_clears_on_artifact(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T1", "a.py", parent_feature="F")]))
    gaps = repo.validate_coverage()
    assert [g.task_name for g in gaps] == ["T1"]

    assert repo.add_artifact("T1", Artifact(path="a.py", kind=ArtifactKind.TEST))
    assert repo.validate_coverage() == []


def test_validate_coverage_clears_on_linked_qa_task(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T1", "a.py", parent_feature="F")]))
    assert len(repo.validate_coverage()) == 1
    repo.push_tasks(AtomicTaskBatch(tasks=[_qa_task("QA1", "a.py", parent_feature="F")]))
    assert repo.validate_coverage() == []


def test_validate_api_contracts_flags_mismatched_schema(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    fe = _dev_task(
        "FE1", "fe.py", parent_feature="F", layer="Frontend",
        api_spec=ApiSpec(endpoint="POST /x", request_schema="{a:int}", response_schema="{ok:bool}"),
    )
    be = _dev_task(
        "BE1", "be.py", parent_feature="F", layer="Backend",
        api_spec=ApiSpec(endpoint="POST /x", request_schema="{a:str}", response_schema="{ok:bool}"),
    )
    repo.push_tasks(AtomicTaskBatch(tasks=[fe, be]))
    violations = repo.validate_api_contracts()
    assert len(violations) == 1
    assert {violations[0].task_a, violations[0].task_b} == {"FE1", "BE1"}


def test_validate_api_contracts_no_violation_when_schemas_agree(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    spec = ApiSpec(endpoint="POST /x", request_schema="{a:int}", response_schema="{ok:bool}")
    fe = _dev_task("FE1", "fe.py", parent_feature="F", layer="Frontend", api_spec=spec)
    be = _dev_task("BE1", "be.py", parent_feature="F", layer="Backend", api_spec=spec)
    repo.push_tasks(AtomicTaskBatch(tasks=[fe, be]))
    assert repo.validate_api_contracts() == []


def test_artifacts_events_round_trip(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T1", "a.py")]))

    assert repo.add_artifact("T1", Artifact(path="a.py", kind=ArtifactKind.SOURCE, verdict="pass"))
    artifacts = repo.get_artifacts("T1")
    assert len(artifacts) == 1 and artifacts[0].verdict == "pass"
    assert repo.artifacts_for_path("a.py")[0][0] == "T1"

    assert repo.add_event("T1", RepairEvent(round=1, failures_before=2, failures_after=0, timestamp="t1"))
    events = repo.get_events("T1")
    assert len(events) == 1 and events[0].failures_after == 0

    assert repo.add_artifact("missing", Artifact(path="x", kind=ArtifactKind.SOURCE)) is False
    assert repo.add_event("missing", RepairEvent()) is False


def test_delete_task_and_delete_all_tasks(tmp_path):
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T1", "a.py")]))
    repo.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T2", "b.py")]), project="proj")

    assert repo.delete_task("T1") is True
    assert repo.get_task("T1") is None
    assert repo.delete_task("does-not-exist") is False

    assert repo.delete_all_tasks("proj") == 1
    assert repo.list_all_for_project("proj") == []


def test_project_scoping_on_a_project_bound_repo(tmp_path):
    db = tmp_path / "tasks.db"
    unscoped = EmbeddedTaskRepository(db)
    unscoped.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T1", "a.py")]), project="proj-a")
    unscoped.push_tasks(AtomicTaskBatch(tasks=[_dev_task("T2b", "b.py")]), project="proj-b")

    scoped = EmbeddedTaskRepository(db, project="proj-a")
    assert [t.name for t in scoped.list_pending()] == ["T1"]
    assert scoped.list_all_for_project("proj-b") == [_dev_and_check(unscoped, "proj-b")]


def _dev_and_check(repo: EmbeddedTaskRepository, project: str) -> AtomicTask:
    tasks = repo.list_all_for_project(project)
    assert len(tasks) == 1
    return tasks[0]


def test_schema_version_matches_the_shared_constant(tmp_path):
    from cie.tasks import SCHEMA_VERSION
    repo = EmbeddedTaskRepository(tmp_path / "tasks.db")
    assert repo.schema_version() == SCHEMA_VERSION
