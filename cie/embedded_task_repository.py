"""Zero-config, embedded task/QA repository — SQLite-backed, no Neo4j.

`docs/growth-plan.md` Phase 0.5, workstream B: task/QA traceability
(`cie.task_repository.TaskRepository`) was Neo4j-only, so the one
capability `docs/competitive-landscape.md` names as unmatched by any
competitor was unreachable from the same two-command zero-config
quickstart that makes cie's code graph itself try-able with no server.
`EmbeddedTaskRepository` closes that gap: the same `TaskRepository`
protocol (it satisfies `isinstance(repo, TaskRepository)` — the protocol
is `@runtime_checkable`, see `test_embedded_task_repository.py`), the
same validation semantics (reuses `cie.task_repository.plan_push`, the
exact function `Neo4jTaskRepository.push_tasks` calls — not a
reimplementation that could quietly drift from it), backed by one local
SQLite file instead of a running Neo4j instance.

**Scope, stated plainly:** every `TaskRepository` protocol method is
implemented for real here, not a subset — this is not another
`NullTaskRepository`. The one deliberate simplification: DEPENDS_ON
"edges" to file paths aren't a separate persisted edge table. A task's
`dependencies` (a list of file-path strings, already part of its own
stored data) *is* the edge — exactly mirroring `Neo4jTaskRepository`'s
own `_write_tasks`, which MERGEs a file node into existence on demand
rather than requiring it to already exist in the code graph. Transitive
traversal (`get_dependencies`/`get_dependent_tasks`/`validate_cycles`)
walks that same data at query time by matching one task's dependency
paths against every other task's `file_path` — see `_dependents_of`
below. Two of the Neo4j implementation's read methods
(`get_dependencies`, `get_dependent_tasks`) never filtered their anchor
task by `project` either (only the traversal's *edges* were naturally
project-scoped, because `_write_tasks` only ever links same-project
tasks) — this implementation matches that literal behavior rather than
quietly tightening it, so the two backends agree.
"""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path
from typing import Optional

from cie.task_repository import (
    _detect_cycle,
    _task_from_node,
    _task_to_props,
    legacy_rejection_message,
    plan_push,
)
from cie.tasks import (
    SCHEMA_VERSION,
    Artifact,
    ArtifactKind,
    AtomicTask,
    AtomicTaskBatch,
    ContractViolation,
    CoverageGap,
    CycleResult,
    PushResult,
    RejectedTask,
    RepairEvent,
    TaskStatus,
)

_SCHEMA = """
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT NOT NULL,
    project TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL,
    props TEXT NOT NULL,
    PRIMARY KEY (id, project)
);
CREATE INDEX IF NOT EXISTS idx_tasks_name ON tasks(name);
CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks(project);

CREATE TABLE IF NOT EXISTS rejected_tasks (
    rowid INTEGER PRIMARY KEY AUTOINCREMENT,
    project TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL,
    reason TEXT NOT NULL,
    userstory_id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    file_path TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    task_name TEXT NOT NULL,
    project TEXT NOT NULL DEFAULT '',
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    commit_sha TEXT NOT NULL DEFAULT '',
    content_ref TEXT NOT NULL DEFAULT '',
    verdict TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (task_name, project, path, kind, commit_sha)
);

CREATE TABLE IF NOT EXISTS events (
    rowid INTEGER PRIMARY KEY AUTOINCREMENT,
    task_name TEXT NOT NULL,
    project TEXT NOT NULL DEFAULT '',
    data TEXT NOT NULL
);
"""


class EmbeddedTaskRepository:
    """`TaskRepository` backed by one local SQLite file.

    Construct one per `.cie/tasks.db` the same way `EmbeddedRepository`
    owns `.cie/graph.db` — the two are independent files, on purpose:
    the code graph and the task graph are separate concerns even when
    both are embedded (see `cie.embedded_repository`'s own docstring for
    the same argument about the code graph).
    """

    def __init__(self, db_path: Path | str, project: str = ""):
        self._db_path = Path(db_path)
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn = sqlite3.connect(str(self._db_path))
        self._conn.executescript(_SCHEMA)
        self._conn.commit()
        self._project = project

    def close(self) -> None:
        self._conn.close()

    # -- internal: row <-> model -------------------------------------------

    def _all_tasks(self, project: str = "") -> list[AtomicTask]:
        """Every stored task, scoped to ``project`` when non-empty, else
        every task regardless of its own stored project — same fallback
        convention `Neo4jTaskRepository._all_tasks` uses."""
        if project:
            cur = self._conn.execute(
                "SELECT props FROM tasks WHERE project = ? ORDER BY name", (project,)
            )
        else:
            cur = self._conn.execute("SELECT props FROM tasks ORDER BY name")
        return [_task_from_node(json.loads(r[0])) for r in cur.fetchall()]

    def _match_task_rows(self, name: str) -> list[tuple[str, AtomicTask]]:
        """(project, task) pairs matching ``name`` — scoped to
        ``self._project`` when set, else every project. Mirrors the
        per-method ``if self._project: ... else: ...`` pattern
        `Neo4jTaskRepository` repeats for get_task/set_status/
        set_qa_status/add_artifact/get_artifacts/add_event/get_events."""
        if self._project:
            cur = self._conn.execute(
                "SELECT project, props FROM tasks WHERE name = ? AND project = ?",
                (name, self._project),
            )
        else:
            cur = self._conn.execute(
                "SELECT project, props FROM tasks WHERE name = ?", (name,)
            )
        return [(r[0], _task_from_node(json.loads(r[1]))) for r in cur.fetchall()]

    # -- A: task supply ------------------------------------------------------

    def list_pending(self) -> list[AtomicTask]:
        """`AtomicTask` itself carries no `status` field — it's a pure
        graph/row write-back property (`_task_to_props`'s docstring),
        never round-tripped back onto the model by `_task_from_node`. So
        this filters on the raw stored props directly, the same way
        `Neo4jTaskRepository.list_pending` filters at the Cypher level
        (`{status: 'pending'}`) rather than on the reconstructed model."""
        if self._project:
            cur = self._conn.execute(
                "SELECT props FROM tasks WHERE project = ? ORDER BY name", (self._project,)
            )
        else:
            cur = self._conn.execute("SELECT props FROM tasks ORDER BY name")
        out = []
        for (raw,) in cur.fetchall():
            props = json.loads(raw)
            if props.get("status", "pending") == TaskStatus.PENDING.value:
                out.append(_task_from_node(props))
        return out

    def list_all_for_project(self, project: str) -> list[AtomicTask]:
        cur = self._conn.execute(
            "SELECT props FROM tasks WHERE project = ? ORDER BY name", (project,)
        )
        return [_task_from_node(json.loads(r[0])) for r in cur.fetchall()]

    def push_tasks(self, batch: AtomicTaskBatch, project: str = "") -> PushResult:
        effective_project = project or self._project
        stored = self._all_tasks(effective_project)
        accepted, rejected = plan_push(batch, stored)
        if accepted:
            self._write_tasks(accepted, project=effective_project)
        if rejected:
            self._write_rejected_tasks(rejected, project=effective_project)
        return PushResult(accepted=len(accepted), rejected=rejected)

    def _write_rejected_tasks(self, rejected: list[RejectedTask], project: str = "") -> None:
        self._conn.executemany(
            "INSERT INTO rejected_tasks (project, name, reason, userstory_id, "
            "task_type, file_path) VALUES (?, ?, ?, ?, ?, ?)",
            [
                (project, r.name, r.reason, r.userstory_id, r.task_type, r.file_path)
                for r in rejected
            ],
        )
        self._conn.commit()

    def submit_batch(self, batch: AtomicTaskBatch) -> int:
        result = self.push_tasks(batch)
        if result.rejected:
            raise ValueError(
                "; ".join(legacy_rejection_message(r) for r in result.rejected)
            )
        return result.accepted

    def _write_tasks(self, tasks: list[AtomicTask], project: str = "") -> None:
        rows = [
            (t.id, project, t.name, json.dumps(_task_to_props(t, project=project)))
            for t in tasks
        ]
        self._conn.executemany(
            "INSERT INTO tasks (id, project, name, props) VALUES (?, ?, ?, ?) "
            "ON CONFLICT (id, project) DO UPDATE SET name = excluded.name, "
            "props = excluded.props",
            rows,
        )
        self._conn.commit()

    def get_task(self, name: str) -> Optional[AtomicTask]:
        matches = self._match_task_rows(name)
        return matches[0][1] if matches else None

    def get_dependencies(self, name: str) -> list[str]:
        """Transitive closure of file-path dependencies. Unscoped by
        project (matches `Neo4jTaskRepository.get_dependencies` literally
        — see module docstring)."""
        task = _first_or_none(self._conn.execute(
            "SELECT props FROM tasks WHERE name = ?", (name,)
        ).fetchall())
        if task is None:
            return []
        start = _task_from_node(json.loads(task[0]))
        by_file = self._by_file_path(project="")
        seen_paths: set[str] = set()
        seen_tasks: set[str] = {start.name}
        stack = list(start.dependencies)
        while stack:
            path = stack.pop()
            if path in seen_paths:
                continue
            seen_paths.add(path)
            for owner in by_file.get(path, []):
                if owner.name not in seen_tasks:
                    seen_tasks.add(owner.name)
                    stack.extend(owner.dependencies)
        return sorted(seen_paths)

    def get_dependent_tasks(self, name: str) -> list[AtomicTask]:
        """Transitive closure over task-to-task DEPENDS_ON edges (a
        dependency path that matches another task's `file_path`).
        Unscoped by project, same rationale as `get_dependencies`."""
        task = _first_or_none(self._conn.execute(
            "SELECT props FROM tasks WHERE name = ?", (name,)
        ).fetchall())
        if task is None:
            return []
        start = _task_from_node(json.loads(task[0]))
        by_file = self._by_file_path(project="")
        out: dict[str, AtomicTask] = {}
        stack = list(start.dependencies)
        seen_paths: set[str] = set()
        while stack:
            path = stack.pop()
            if path in seen_paths:
                continue
            seen_paths.add(path)
            for owner in by_file.get(path, []):
                if owner.name != start.name and owner.name not in out:
                    out[owner.name] = owner
                    stack.extend(owner.dependencies)
        return sorted(out.values(), key=lambda t: t.name)

    def _by_file_path(self, project: str) -> dict[str, list[AtomicTask]]:
        by_file: dict[str, list[AtomicTask]] = {}
        for t in self._all_tasks(project):
            by_file.setdefault(t.file_path, []).append(t)
        return by_file

    def artifacts_for_path(self, path: str) -> list[tuple[str, Artifact]]:
        cur = self._conn.execute(
            "SELECT task_name, path, kind, commit_sha, content_ref, verdict "
            "FROM artifacts WHERE path = ? ORDER BY task_name",
            (path,),
        )
        return [(r[0], _row_to_artifact(r)) for r in cur.fetchall()]

    def schema_version(self) -> str:
        return SCHEMA_VERSION

    # -- B: status write-back -------------------------------------------------

    def set_status(self, name: str, status: TaskStatus, updated_at: str) -> bool:
        matches = self._match_task_rows(name)
        if not matches:
            return False
        for project, task in matches:
            props = _task_to_props(task, project=project)
            props["status"] = status.value
            props["updated"] = updated_at
            props["attempts"] = int(props.get("attempts") or 0) + 1
            self._conn.execute(
                "UPDATE tasks SET props = ? WHERE id = ? AND project = ?",
                (json.dumps(props), task.id, project),
            )
        self._conn.commit()
        return True

    def set_status_cached(self, name: str, status: TaskStatus, updated_at: str) -> None:
        # No write-behind cache in the embedded backend — a local SQLite
        # write is already cheap enough that the batching optimization
        # `Neo4jTaskRepository` needs for network round-trips doesn't
        # apply here. Same synchronous behavior as set_status.
        self.set_status(name, status, updated_at)

    def set_qa_status(self, name: str, qa_status: str, updated_at: str) -> bool:
        matches = self._match_task_rows(name)
        if not matches:
            return False
        for project, task in matches:
            props = _task_to_props(task, project=project)
            props["qa_status"] = qa_status
            props["qa_updated"] = updated_at
            self._conn.execute(
                "UPDATE tasks SET props = ? WHERE id = ? AND project = ?",
                (json.dumps(props), task.id, project),
            )
        self._conn.commit()
        return True

    def add_artifact(self, task_name: str, artifact: Artifact) -> bool:
        matches = self._match_task_rows(task_name)
        if not matches:
            return False
        for project, _task in matches:
            self._conn.execute(
                "INSERT INTO artifacts (task_name, project, path, kind, "
                "commit_sha, content_ref, verdict) VALUES (?, ?, ?, ?, ?, ?, ?) "
                "ON CONFLICT (task_name, project, path, kind, commit_sha) "
                "DO UPDATE SET content_ref = excluded.content_ref, "
                "verdict = excluded.verdict",
                (
                    task_name, project, artifact.path, artifact.kind.value,
                    artifact.commit_sha, artifact.content_ref, artifact.verdict,
                ),
            )
        self._conn.commit()
        return True

    def get_artifacts(self, task_name: str) -> list[Artifact]:
        project = self._project
        if project:
            cur = self._conn.execute(
                "SELECT task_name, path, kind, commit_sha, content_ref, verdict "
                "FROM artifacts WHERE task_name = ? AND project = ? ORDER BY path",
                (task_name, project),
            )
        else:
            cur = self._conn.execute(
                "SELECT task_name, path, kind, commit_sha, content_ref, verdict "
                "FROM artifacts WHERE task_name = ? ORDER BY path",
                (task_name,),
            )
        return [_row_to_artifact(r) for r in cur.fetchall()]

    def get_events(self, task_name: str) -> list[RepairEvent]:
        project = self._project
        if project:
            cur = self._conn.execute(
                "SELECT data FROM events WHERE task_name = ? AND project = ?",
                (task_name, project),
            )
        else:
            cur = self._conn.execute(
                "SELECT data FROM events WHERE task_name = ?", (task_name,)
            )
        events = [RepairEvent(**json.loads(r[0])) for r in cur.fetchall()]
        return sorted(events, key=lambda e: e.timestamp)

    def add_event(self, task_name: str, event: RepairEvent) -> bool:
        matches = self._match_task_rows(task_name)
        if not matches:
            return False
        payload = json.dumps(event.model_dump())
        for project, _task in matches:
            self._conn.execute(
                "INSERT INTO events (task_name, project, data) VALUES (?, ?, ?)",
                (task_name, project, payload),
            )
        self._conn.commit()
        return True

    # -- E: deletion -----------------------------------------------------------

    def delete_task(self, name: str, project: str = "") -> bool:
        if project:
            cur = self._conn.execute(
                "DELETE FROM tasks WHERE name = ? AND project = ?", (name, project)
            )
        else:
            cur = self._conn.execute("DELETE FROM tasks WHERE name = ?", (name,))
        self._conn.commit()
        return cur.rowcount > 0

    def delete_tasks(self, names: list[str], project: str = "") -> int:
        if not names:
            return 0
        placeholders = ",".join("?" for _ in names)
        if project:
            cur = self._conn.execute(
                f"DELETE FROM tasks WHERE name IN ({placeholders}) AND project = ?",
                (*names, project),
            )
        else:
            cur = self._conn.execute(
                f"DELETE FROM tasks WHERE name IN ({placeholders})", tuple(names)
            )
        self._conn.commit()
        return cur.rowcount

    def delete_all_tasks(self, project: str) -> int:
        cur = self._conn.execute("DELETE FROM tasks WHERE project = ?", (project,))
        self._conn.commit()
        return cur.rowcount

    # -- D: consistency validation ----------------------------------------------

    def _scoped_tasks(self) -> list[AtomicTask]:
        return self._all_tasks(self._project)

    def validate_cycles(self) -> CycleResult:
        tasks = self._scoped_tasks()
        by_file: dict[str, list[str]] = {}
        for t in tasks:
            by_file.setdefault(t.file_path, []).append(t.name)
        deps: dict[str, list[str]] = {}
        for t in tasks:
            owners: list[str] = []
            for path in t.dependencies:
                for owner in by_file.get(path, []):
                    if owner not in owners:
                        owners.append(owner)
            deps[t.name] = owners
        cycle = _detect_cycle([t.name for t in tasks], deps)
        return CycleResult(has_cycle=bool(cycle), cycle=cycle)

    def validate_coverage(self) -> list[CoverageGap]:
        """Dev tasks with no test artifact and no linked QA task (same
        parent_feature + file_path, task_type == 'qa')."""
        tasks = self._scoped_tasks()
        dev_tasks = [t for t in tasks if t.task_type == "dev"]
        qa_pairs = {
            (t.parent_feature, t.file_path) for t in tasks if t.task_type == "qa"
        }
        project = self._project
        if project:
            test_rows = self._conn.execute(
                "SELECT DISTINCT task_name FROM artifacts "
                "WHERE project = ? AND kind = ?",
                (project, ArtifactKind.TEST.value),
            ).fetchall()
        else:
            test_rows = self._conn.execute(
                "SELECT DISTINCT task_name FROM artifacts WHERE kind = ?",
                (ArtifactKind.TEST.value,),
            ).fetchall()
        has_test = {r[0] for r in test_rows}
        return [
            CoverageGap(task_name=t.name, file_path=t.file_path, parent_feature=t.parent_feature)
            for t in dev_tasks
            if t.name not in has_test and (t.parent_feature, t.file_path) not in qa_pairs
        ]

    def validate_api_contracts(self) -> list[ContractViolation]:
        """Paired FE/BE tasks (same parent_feature) that disagree on a
        shared API spec endpoint's request/response schema."""
        tasks = self._scoped_tasks()
        fe_tasks = [t for t in tasks if t.layer == "Frontend" and t.api_spec]
        be_tasks = [t for t in tasks if t.layer in ("API", "Backend") and t.api_spec]
        violations: list[ContractViolation] = []
        for fe in fe_tasks:
            for be in be_tasks:
                if fe.parent_feature != be.parent_feature:
                    continue
                if fe.api_spec.endpoint != be.api_spec.endpoint:
                    continue
                if (
                    fe.api_spec.request_schema != be.api_spec.request_schema
                    or fe.api_spec.response_schema != be.api_spec.response_schema
                ):
                    violations.append(ContractViolation(
                        task_a=fe.name, task_b=be.name,
                        endpoint=fe.api_spec.endpoint, parent_feature=fe.parent_feature,
                        reason="mismatched request/response schema",
                    ))
        return violations


def _first_or_none(rows):
    return rows[0] if rows else None


def _row_to_artifact(row) -> Artifact:
    task_name, path, kind, commit_sha, content_ref, verdict = row
    try:
        artifact_kind = ArtifactKind(kind)
    except ValueError:
        artifact_kind = ArtifactKind.SOURCE
    return Artifact(
        path=path, kind=artifact_kind, commit_sha=commit_sha,
        content_ref=content_ref, verdict=verdict,
    )
