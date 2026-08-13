"""Task repository protocol + Neo4j implementation.

Separate protocol from the code-graph Repository, as suggested in the
integration spec: task CRUD and validation live here, code-structure queries
live in cie.repository. Both share the same Neo4j driver and the same
file nodes (DEPENDS_ON edges point at code-graph file nodes AND at other
AtomicTask nodes whose file_path matches, unifying the two graphs).
"""

from __future__ import annotations

import json
import logging
from typing import Optional, Protocol, runtime_checkable

log = logging.getLogger(__name__)

from cie.tasks import (
    SCHEMA_VERSION,
    Artifact,
    ArtifactKind,
    AtomicQaTask,
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


@runtime_checkable
class TaskRepository(Protocol):
    """Persistence and validation contract for the forge task pipeline."""

    # -- A: task supply ----------------------------------------------------

    def list_pending(self) -> list[AtomicTask]:
        """Return all pending tasks ordered by creation time."""

    def list_all_for_project(self, project: str) -> list[AtomicTask]:
        """Return every task (any status) stamped with ``project``,
        ordered by creation time. Unlike :meth:`list_pending`, this is
        project-scoped — see :meth:`push_tasks`'s ``project`` param for how
        that property gets onto the node in the first place."""

    def push_tasks(self, batch: AtomicTaskBatch, project: str = "") -> PushResult:
        """Validate and merge a batch of tasks with PER-TASK partial accept.

        Never raises on a bad task: schema violations, dev tasks without a
        test_triad, API-layer tasks without an api_spec, duplicate names within
        the batch, and dependency cycles (batch-internal or against stored
        tasks) are collected into ``rejected`` with a precise reason; every
        valid task is written. A rejected task writes nothing.
        """

    def submit_batch(self, batch: AtomicTaskBatch) -> int:
        """Back-compat alias for :meth:`push_tasks`.

        Returns the count written; raises ``ValueError`` when ANY task is
        rejected (legacy all-or-nothing behavior). New callers should use
        :meth:`push_tasks` for the partial-accept contract.
        """

    def get_task(self, name: str) -> Optional[AtomicTask]:
        """Return a single task by name, or None."""

    def get_dependencies(self, name: str) -> list[str]:
        """Return the transitive closure of file-path dependencies for a task."""

    def get_dependent_tasks(self, name: str) -> list[AtomicTask]:
        """Return the transitive closure of task-to-task DEPENDS_ON deps.

        This is the set of tasks ``name`` (transitively) depends on — the
        forge topo-sort feed. Empty list when the task is unknown.
        """

    def artifacts_for_path(self, path: str) -> list[tuple[str, Artifact]]:
        """Return ``(task_name, artifact)`` pairs for artifacts at ``path``.

        Powers the blame_history join ("which tasks touched this file").
        """

    def schema_version(self) -> str:
        """Return the atomic-task schema version enforced at ingest."""

    # -- B: status write-back ---------------------------------------------

    def set_status(self, name: str, status: TaskStatus, updated_at: str) -> bool:
        """Update a task's status and bump attempts. Return False if missing."""

    def set_qa_status(self, name: str, qa_status: str, updated_at: str) -> bool:
        """Update a task's QA-specific outcome ('qa_generated'/'qa_failed').

        Separate from :meth:`set_status`: `status` tracks a task's own
        generate/repair lifecycle (shared by dev and qa tasks alike —
        see forge/generate_agent.py), while `qa_status` is the outcome
        of a QA-type task's own generation specifically, surfaced by the
        dashboard's "QA Gen" counters. Not a TaskStatus enum member since
        it's a distinct, smaller vocabulary with no PENDING/TESTED_GREEN
        states. Return False if the task is missing."""

    def add_artifact(self, task_name: str, artifact: Artifact) -> bool:
        """Link a produced artifact to a task. Return False if task missing."""

    def add_event(self, task_name: str, event: RepairEvent) -> bool:
        """Append a repair event to a task. Return False if task missing."""

    # -- D: consistency validation ----------------------------------------

    def validate_cycles(self) -> CycleResult:
        """Detect cycles in the DEPENDS_ON graph."""

    def validate_coverage(self) -> list[CoverageGap]:
        """Find dev tasks with no test artifact and no linked QA task."""

    def validate_api_contracts(self) -> list[ContractViolation]:
        """Find paired FE/BE tasks that disagree on a shared API spec."""


# ---------------------------------------------------------------------------
# Serialization helpers (pydantic models ↔ Neo4j properties)
# ---------------------------------------------------------------------------


def _task_from_node(props: dict) -> AtomicTask:
    """Build an AtomicTask from a Neo4j node's properties — an
    AtomicQaTask specifically when task_type == 'qa', so its 7
    test-execution-metadata fields (test_framework/run_command/test_type/
    test_origin/expected_assertions/requires_mocks/tested_file_path)
    survive the round trip instead of being silently dropped by plain
    AtomicTask's schema (confirmed live: every QA task read back through
    this function came out shaped exactly like a dev task)."""
    api_spec = None
    api_json = props.get("api_spec_json")
    if api_json:
        api_spec = ApiSpec.model_validate_json(api_json)
    test_triad = None
    triad_json = props.get("test_triad_json")
    if triad_json:
        test_triad = TestTriad.model_validate_json(triad_json)
    common = dict(
        id=props.get("id", ""),
        name=props.get("name", ""),
        userstory_id=props.get("userstory_id", ""),
        parent_feature=props.get("parent_feature", ""),
        task_type=props.get("task_type", ""),
        layer=props.get("layer", ""),
        action=props.get("action", ""),
        file_path=props.get("file_path", ""),
        description=props.get("description", ""),
        exact_imports=json.loads(props.get("exact_imports_json", "[]")),
        function_signatures=json.loads(props.get("function_signatures_json", "[]")),
        step_by_step_implementation=json.loads(props.get("step_by_step_json", "[]")),
        dependencies=json.loads(props.get("dependencies_json", "[]")),
        api_spec=api_spec,
        test_triad=test_triad,
        origin=props.get("origin", ""),
        dev_task_id=props.get("dev_task_id", ""),
        preconditions=json.loads(props.get("preconditions_json", "[]")),
        postconditions=json.loads(props.get("postconditions_json", "[]")),
    )
    if props.get("task_type") != "qa":
        return AtomicTask(**common)
    return AtomicQaTask(
        **common,
        tested_file_path=props.get("tested_file_path", ""),
        test_framework=props.get("test_framework") or None,
        run_command=props.get("run_command") or None,
        test_type=props.get("test_type") or None,
        test_origin=props.get("test_origin") or None,
        expected_assertions=json.loads(props.get("expected_assertions_json", "[]")),
        requires_mocks=bool(props.get("requires_mocks", False)),
    )


def _task_to_props(t: AtomicTask, project: str = "") -> dict:
    """Convert an AtomicTask to Neo4j node properties.

    ``project`` is a graph-only write-back property (like status/attempts),
    not part of AtomicTask's own pydantic schema — same pattern the module
    docstring already documents for status/attempts/created/updated. Only
    set when non-empty so existing unscoped callers/tests keep writing
    exactly the properties they did before.

    When ``t`` is an AtomicQaTask, its 7 test-execution-metadata fields
    are written too (companion half of _task_from_node's fix above) — a
    plain AtomicTask has none of these attrs, so dev tasks keep writing
    exactly the same property set as before."""
    props = {
        "id": t.id,
        "name": t.name,
        "userstory_id": t.userstory_id,
        "parent_feature": t.parent_feature,
        "task_type": t.task_type,
        "layer": t.layer,
        "action": t.action,
        "file_path": t.file_path,
        "description": t.description,
        "exact_imports_json": json.dumps(t.exact_imports),
        "function_signatures_json": json.dumps(t.function_signatures),
        "step_by_step_json": json.dumps(t.step_by_step_implementation),
        "dependencies_json": json.dumps(t.dependencies),
        "api_spec_json": t.api_spec.model_dump_json() if t.api_spec else "",
        "test_triad_json": t.test_triad.model_dump_json() if t.test_triad else "",
        "origin": t.origin,
        "dev_task_id": t.dev_task_id,
        "preconditions_json": json.dumps(t.preconditions),
        "postconditions_json": json.dumps(t.postconditions),
        "status": "pending",
        "attempts": 0,
        "created": "",
        "updated": "",
        "qa_status": "",
        "qa_updated": "",
    }
    if isinstance(t, AtomicQaTask):
        props.update({
            "tested_file_path": t.tested_file_path,
            "test_framework": t.test_framework.value if t.test_framework else "",
            "run_command": t.run_command or "",
            "test_type": t.test_type.value if t.test_type else "",
            "test_origin": t.test_origin.value if t.test_origin else "",
            "expected_assertions_json": json.dumps(t.expected_assertions),
            "requires_mocks": t.requires_mocks,
        })
    if project:
        props["project"] = project
    return props


def _detect_cycle(names: list[str], deps: dict[str, list[str]]) -> list[str]:
    """Return the first cycle found (list of task names), or empty list."""
    WHITE, GRAY, BLACK = 0, 1, 2
    color = {n: WHITE for n in names}
    stack: list[str] = []

    def dfs(node: str) -> list[str]:
        color[node] = GRAY
        stack.append(node)
        for dep in deps.get(node, []):
            if dep not in color:
                continue
            if color[dep] == GRAY:
                idx = stack.index(dep)
                return stack[idx:]
            if color[dep] == WHITE:
                found = dfs(dep)
                if found:
                    return found
        stack.pop()
        color[node] = BLACK
        return []

    for n in names:
        if color[n] == WHITE:
            found = dfs(n)
            if found:
                return found
    return []


# ---------------------------------------------------------------------------
# Partial-accept push validation (shared by Neo4j impl and test fakes)
# ---------------------------------------------------------------------------

_VALID_TASK_TYPES = ("dev", "qa")
_VALID_ACTIONS = ("create", "modify", "delete")
_VALID_LAYERS = ("", "Backend", "Frontend", "API", "Data")


def _task_schema_error(t: AtomicTask) -> Optional[str]:
    """Return a precise reason when a task violates the schema rules, else None.

    These are the constraints pydantic cannot express on the plain ``str``
    fields (enums documented in the field descriptions, non-empty identity
    fields).
    """
    if not t.name.strip():
        return "task name is empty"
    if not t.file_path.strip():
        return f"task '{t.name}' has an empty file_path"
    if t.task_type not in _VALID_TASK_TYPES:
        return (
            f"task '{t.name}' has invalid task_type '{t.task_type}'"
            f" (expected one of {list(_VALID_TASK_TYPES)})"
        )
    if t.action not in _VALID_ACTIONS:
        return (
            f"task '{t.name}' has invalid action '{t.action}'"
            f" (expected one of {list(_VALID_ACTIONS)})"
        )
    if t.layer not in _VALID_LAYERS:
        return (
            f"task '{t.name}' has invalid layer '{t.layer}'"
            f" (expected one of {[x for x in _VALID_LAYERS if x]})"
        )
    return None


def _task_rule_error(t: AtomicTask) -> Optional[str]:
    """Return a precise reason when a task violates the pipeline invariants."""
    if t.task_type == "dev" and t.test_triad is None:
        return f"dev task '{t.name}' is missing a test_triad"
    if t.layer == "API" and t.api_spec is None:
        return f"API-layer task '{t.name}' is missing an api_spec"
    return None


def _dependency_graph(
    stored: list[AtomicTask], candidates: list[AtomicTask]
) -> dict[str, list[str]]:
    """Build the task-to-task dependency graph (name -> dependency task names).

    A task depends on every stored/candidate task whose ``file_path`` appears
    in its ``dependencies`` list (all matches, not just the first).
    """
    by_file: dict[str, list[str]] = {}
    for t in [*stored, *candidates]:
        by_file.setdefault(t.file_path, []).append(t.name)
    graph: dict[str, list[str]] = {}
    for t in [*stored, *candidates]:
        deps: list[str] = []
        for path in t.dependencies:
            for owner in by_file.get(path, []):
                if owner not in deps:
                    deps.append(owner)
        graph[t.name] = deps
    return graph


def _cycle_through(node: str, deps: dict[str, list[str]]) -> list[str]:
    """Return one witness cycle starting and ending at ``node``, or [].

    The path is returned as a name list, e.g. ``["a", "b", "a"]``.
    """
    for start in deps.get(node, []):
        if start == node:
            return [node, node]
        stack: list[tuple[str, list[str]]] = [(start, [node, start])]
        visited: set[str] = set()
        while stack:
            current, path = stack.pop()
            for nxt in deps.get(current, []):
                if nxt == node:
                    return path + [node]
                if nxt not in visited:
                    visited.add(nxt)
                    stack.append((nxt, path + [nxt]))
    return []


def plan_push(
    batch: AtomicTaskBatch, stored: list[AtomicTask]
) -> tuple[list[AtomicTask], list[RejectedTask]]:
    """Split a batch into (accepted, rejected) under the partial-accept rules.

    Rejection rules, per task, in order:
      1. duplicate name within the batch,
      2. schema violations (enums, empty name/file_path),
      3. dev-without-triad / API-layer-without-api_spec,
      4. dependency cycles (batch-internal or against ``stored`` tasks).

    A task is only cycle-checked against tasks that will actually exist
    (stored + otherwise-valid batchmates), so rejecting one member of a would-be
    cycle for another reason clears the cycle for the rest.
    """
    rejected: list[RejectedTask] = []
    survivors: list[AtomicTask] = []
    seen: set[str] = set()
    for t in batch.tasks:
        reason: Optional[str] = None
        if t.name in seen:
            reason = f"duplicate task name '{t.name}' in batch"
        if reason is None:
            reason = _task_schema_error(t)
        if reason is None:
            reason = _task_rule_error(t)
        if reason is not None:
            rejected.append(RejectedTask(
                name=t.name, reason=reason, userstory_id=t.userstory_id,
                task_type=t.task_type, file_path=t.file_path,
            ))
        else:
            seen.add(t.name)
            survivors.append(t)
    # Cycle check over the graph that would exist after writing the survivors.
    graph = _dependency_graph(stored, survivors)
    still_valid: list[AtomicTask] = []
    for t in survivors:
        cycle = _cycle_through(t.name, graph)
        if cycle:
            rejected.append(
                RejectedTask(
                    name=t.name, reason=f"dependency cycle: {' -> '.join(cycle)}",
                    userstory_id=t.userstory_id, task_type=t.task_type,
                    file_path=t.file_path,
                )
            )
        else:
            still_valid.append(t)
    return still_valid, rejected


def legacy_rejection_message(rejected: RejectedTask) -> str:
    """Render a rejection in the legacy ``submit_batch`` error style."""
    if rejected.reason.startswith("dependency cycle: "):
        cycle = rejected.reason[len("dependency cycle: "):]
        return f"cyclic dependencies detected: {cycle}"
    return rejected.reason


# Late imports for pydantic models used in helpers
from cie.tasks import ApiSpec, TestTriad  # noqa: E402


class Neo4jTaskRepository:
    """Neo4j-backed implementation of the TaskRepository protocol.

    Dependencies are file paths. DEPENDS_ON edges point at code-graph file
    nodes (created on demand if the loader hasn't seen them yet) AND at other
    AtomicTask nodes whose file_path matches a dependency, unifying the task
    graph with the code graph. Task-to-task edges feed the transitive-closure
    queries (get_dependent_tasks, validate_cycles).
    """

    def __init__(self, driver, project: str = ""):
        self._driver = driver
        #: Scopes list_pending() to this project — see that method.
        #: factory.get_task_repo(project) always passes this through, so
        #: every caller going through the factory (forge_repair routes,
        #: forge/graph_source.pull_batch_cie, the CLI, cie's own
        #: tools) is scoped for free.
        self._project = project

    @classmethod
    def from_driver(cls, driver, project: str = "") -> "Neo4jTaskRepository":
        return cls(driver, project=project)

    def _run(self, query: str, params: Optional[dict] = None):
        with self._driver.session() as session:
            return list(session.run(query, params or {}))

    # -- A: task supply ----------------------------------------------------

    def list_pending(self) -> list[AtomicTask]:
        """Pending tasks, scoped to this repo's project when one was given
        at construction. Confirmed live: unscoped, this MATCHed every
        pending :AtomicTask in the shared Neo4j regardless of project —
        "Run Forge" on one project pulled in another project's tasks (and
        any legacy/corrupted task lacking a real task_type), hard-crashing
        the whole batch on the first bad record. project="" (no repo
        constructed with a project) preserves the old unfiltered behavior
        — only callers that go through factory.get_task_repo(project) with
        a real project get scoping."""
        if self._project:
            rows = self._run(
                "MATCH (t:AtomicTask {status: 'pending', project: $project}) "
                "RETURN t ORDER BY t.created, t.name",
                {"project": self._project},
            )
        else:
            rows = self._run(
                "MATCH (t:AtomicTask {status: 'pending'}) "
                "RETURN t ORDER BY t.created, t.name"
            )
        return [_task_from_node(dict(r["t"])) for r in rows]

    def _all_tasks(self) -> list[AtomicTask]:
        """Return every stored task (used for stored-vs-batch cycle checks)."""
        rows = self._run("MATCH (t:AtomicTask) RETURN t")
        return [_task_from_node(dict(r["t"])) for r in rows]

    def list_all_for_project(self, project: str) -> list[AtomicTask]:
        """Return every task stamped with ``project`` (any status), ordered
        by creation. Only sees tasks written via a ``push_tasks(...,
        project=...)`` call — pre-existing/unscoped tasks (no ``project``
        property at all) never match."""
        rows = self._run(
            "MATCH (t:AtomicTask {project: $project}) "
            "RETURN t ORDER BY t.created, t.name",
            {"project": project},
        )
        return [_task_from_node(dict(r["t"])) for r in rows]

    def push_tasks(self, batch: AtomicTaskBatch, project: str = "") -> PushResult:
        """Validate per task and write only the valid ones (partial accept).

        Rejections are now durably recorded (`:RejectedTask` nodes, one per
        rejection, append-only — never MERGEd/deduplicated, since each is a
        distinct historical event, not an entity) — previously the ONLY
        record of a rejected task was a `print()` in the caller
        (features/mine/service.py), which produced exactly the failure
        mode confirmed live 2026-08-13 on book-my-calender: a dev task
        silently dropped for missing a test_triad, its sibling QA task
        written anyway (no equivalent gate), leaving an orphaned test with
        no implementation and nothing anywhere to show it had ever
        happened. Never blocks the push itself — a durability failure here
        must not turn a successful partial-accept into a hard error."""
        accepted, rejected = plan_push(batch, self._all_tasks())
        if accepted:
            self._write_tasks(accepted, project=project)
        if rejected:
            try:
                self._write_rejected_tasks(rejected, project=project)
            except Exception:  # noqa: BLE001 — best-effort durability, never fatal
                log.exception(
                    "failed to durably record %d rejected task(s) for project %r",
                    len(rejected), project,
                )
        return PushResult(accepted=len(accepted), rejected=rejected)

    def _write_rejected_tasks(self, rejected: list[RejectedTask], project: str = "") -> None:
        with self._driver.session() as session:
            for r in rejected:
                session.run(
                    "CREATE (n:RejectedTask {id: randomUUID(), project: $project, "
                    "name: $name, reason: $reason, userstory_id: $userstory_id, "
                    "task_type: $task_type, file_path: $file_path, "
                    "created: datetime()})",
                    {
                        "project": project, "name": r.name, "reason": r.reason,
                        "userstory_id": r.userstory_id, "task_type": r.task_type,
                        "file_path": r.file_path,
                    },
                )

    def submit_batch(self, batch: AtomicTaskBatch) -> int:
        """Back-compat alias: all-or-nothing wrapper around :meth:`push_tasks`.

        Raises ``ValueError`` (legacy message style) when any task is
        rejected; otherwise returns the number of tasks written.
        """
        result = self.push_tasks(batch)
        if result.rejected:
            raise ValueError(
                "; ".join(legacy_rejection_message(r) for r in result.rejected)
            )
        return result.accepted

    def _write_tasks(self, tasks: list[AtomicTask], project: str = "") -> None:
        """Merge task nodes plus ApiSpec/TestTriad/file/task DEPENDS_ON edges."""
        with self._driver.session() as session:
            for t in tasks:
                props = _task_to_props(t, project=project)
                session.run(
                    "MERGE (t:AtomicTask {name: $name}) SET t += $props",
                    {"name": t.name, "props": props},
                )
                # Write ApiSpec as a separate node if present
                if t.api_spec:
                    session.run(
                        "MATCH (t:AtomicTask {name: $name}) "
                        "MERGE (s:ApiSpec {endpoint: $ep, request_schema: $req, response_schema: $res}) "
                        "SET s.error_codes_json = $errs "
                        "MERGE (t)-[:OWNS_API]->(s)",
                        {
                            "name": t.name,
                            "ep": t.api_spec.endpoint,
                            "req": t.api_spec.request_schema,
                            "res": t.api_spec.response_schema,
                            "errs": json.dumps(t.api_spec.error_codes),
                        },
                    )
                # Write TestTriad as a separate node if present
                if t.test_triad:
                    session.run(
                        "MATCH (t:AtomicTask {name: $name}) "
                        "MERGE (tri:TestTriad {positive: $p, negative: $n, negative_to_positive: $ntp}) "
                        "MERGE (t)-[:OWNS_TRIAD]->(tri)",
                        {
                            "name": t.name,
                            "p": t.test_triad.positive,
                            "n": t.test_triad.negative,
                            "ntp": t.test_triad.negative_to_positive,
                        },
                    )
                # Write DEPENDS_ON edges to file nodes (unified code graph)
                for dep_path in t.dependencies:
                    session.run(
                        "MATCH (t:AtomicTask {name: $name}) "
                        "MERGE (f:Node {kind: 'file', source_file: $path}) "
                        "ON CREATE SET f.label = $path, f.id = randomUUID() "
                        "MERGE (t)-[:DEPENDS_ON]->(f)",
                        {"name": t.name, "path": dep_path},
                    )
            # Task-to-task DEPENDS_ON edges (batch-internal + against stored
            # tasks) — written after all task nodes so batch deps resolve.
            for t in tasks:
                for dep_path in t.dependencies:
                    session.run(
                        "MATCH (t:AtomicTask {name: $name}), "
                        "(d:AtomicTask {file_path: $path}) "
                        "MERGE (t)-[:DEPENDS_ON]->(d)",
                        {"name": t.name, "path": dep_path},
                    )

    def get_task(self, name: str) -> Optional[AtomicTask]:
        rows = self._run(
            "MATCH (t:AtomicTask {name: $name}) RETURN t", {"name": name}
        )
        if not rows:
            return None
        return _task_from_node(dict(rows[0]["t"]))

    def get_dependencies(self, name: str) -> list[str]:
        """Return the transitive closure of file-path dependencies."""
        rows = self._run(
            "MATCH (t:AtomicTask {name: $name})-[:DEPENDS_ON*1..]->(f:Node) "
            "RETURN DISTINCT f.source_file AS path",
            {"name": name},
        )
        return [r["path"] for r in rows]

    def get_dependent_tasks(self, name: str) -> list[AtomicTask]:
        """Return the transitive closure over task-to-task DEPENDS_ON edges."""
        rows = self._run(
            "MATCH (t:AtomicTask {name: $name})-[:DEPENDS_ON*1..]->(d:AtomicTask) "
            "RETURN DISTINCT d ORDER BY d.name",
            {"name": name},
        )
        return [_task_from_node(dict(r["d"])) for r in rows]

    def artifacts_for_path(self, path: str) -> list[tuple[str, Artifact]]:
        """Return (task_name, artifact) pairs for artifacts stored at ``path``."""
        rows = self._run(
            "MATCH (t:AtomicTask)-[:PRODUCED]->(a:Artifact {path: $path}) "
            "RETURN t.name AS task_name, a ORDER BY t.name",
            {"path": path},
        )
        out: list[tuple[str, Artifact]] = []
        for r in rows:
            props = dict(r["a"])
            kind = props.get("kind", ArtifactKind.SOURCE.value)
            try:
                artifact_kind = ArtifactKind(kind)
            except ValueError:
                artifact_kind = ArtifactKind.SOURCE
            out.append(
                (
                    r["task_name"],
                    Artifact(
                        path=props.get("path", path),
                        kind=artifact_kind,
                        commit_sha=props.get("commit_sha", ""),
                    ),
                )
            )
        return out

    def schema_version(self) -> str:
        """Return the atomic-task schema version enforced at ingest."""
        return SCHEMA_VERSION

    # -- B: status write-back ---------------------------------------------

    def set_status(self, name: str, status: TaskStatus, updated_at: str) -> bool:
        rows = self._run(
            "MATCH (t:AtomicTask {name: $name}) "
            "SET t.status = $status, t.updated = $updated, t.attempts = t.attempts + 1 "
            "RETURN count(t) AS c",
            {"name": name, "status": status.value, "updated": updated_at},
        )
        return bool(rows and rows[0]["c"] > 0)

    def set_qa_status(self, name: str, qa_status: str, updated_at: str) -> bool:
        rows = self._run(
            "MATCH (t:AtomicTask {name: $name}) "
            "SET t.qa_status = $qa_status, t.qa_updated = $updated "
            "RETURN count(t) AS c",
            {"name": name, "qa_status": qa_status, "updated": updated_at},
        )
        return bool(rows and rows[0]["c"] > 0)

    def add_artifact(self, task_name: str, artifact: Artifact) -> bool:
        rows = self._run(
            "MATCH (t:AtomicTask {name: $name}) "
            "MERGE (a:Artifact {path: $path, kind: $kind, commit_sha: $commit}) "
            # content_ref/verdict (docs/forge-rebuild-plan.md's Phase 1 /
            # WS9): SET, not part of the MERGE key, so repeated
            # record()/add_artifact() calls for the same path (typically
            # kind=SOURCE, commit_sha="") update the SAME artifact node's
            # verdict in place instead of creating a new one per call.
            "SET a.content_ref = $content_ref, a.verdict = $verdict "
            "MERGE (t)-[:PRODUCED]->(a) "
            "RETURN count(t) AS c",
            {
                "name": task_name,
                "path": artifact.path,
                "kind": artifact.kind.value,
                "commit": artifact.commit_sha,
                "content_ref": artifact.content_ref,
                "verdict": artifact.verdict,
            },
        )
        return bool(rows and rows[0]["c"] > 0)

    def add_event(self, task_name: str, event: RepairEvent) -> bool:
        props: dict = {
            "round": event.round,
            "failures_before": event.failures_before,
            "failures_after": event.failures_after,
            "mode": event.mode,
            "lint_ok": event.lint_ok,
            "timestamp": event.timestamp,
        }
        # Legacy fields, stored verbatim when provided.
        if event.failures is not None:
            props["failures"] = event.failures
        if event.ts is not None:
            props["ts"] = event.ts
        rows = self._run(
            "MATCH (t:AtomicTask {name: $name}) "
            "CREATE (e:RepairEvent $props) "
            "MERGE (t)-[:HAS_EVENT]->(e) "
            "RETURN count(t) AS c",
            {"name": task_name, "props": props},
        )
        return bool(rows and rows[0]["c"] > 0)

    # -- E: deletion --------------------------------------------------------

    def delete_task(self, name: str, project: str = "") -> bool:
        """Delete a single AtomicTask node and its attached sub-nodes/edges.

        Removes the task's OWNS_API/OWNS_TRIAD/PRODUCED/HAS_EVENT/DEPENDS_ON
        edges and the sub-nodes that become orphaned (ApiSpec, TestTriad,
        Artifact, RepairEvent). Returns True if a task was deleted.
        """
        params: dict = {"name": name}
        project_filter = ""
        if project:
            params["project"] = project
            project_filter = " AND t.project = $project"
        rows = self._run(
            f"MATCH (t:AtomicTask {{name: $name}}){project_filter} "
            "DETACH DELETE t "
            "RETURN count(t) AS c",
            params,
        )
        return bool(rows and rows[0]["c"] > 0)

    def delete_tasks(self, names: list[str], project: str = "") -> int:
        """Delete multiple AtomicTask nodes by name. Returns the count deleted."""
        if not names:
            return 0
        params: dict = {"names": names}
        project_filter = ""
        if project:
            params["project"] = project
            project_filter = " AND t.project = $project"
        rows = self._run(
            f"MATCH (t:AtomicTask) WHERE t.name IN $names{project_filter} "
            "DETACH DELETE t "
            "RETURN count(t) AS c",
            params,
        )
        return int(rows[0]["c"]) if rows else 0

    def delete_all_tasks(self, project: str) -> int:
        """Delete every AtomicTask for a project. Returns the count deleted."""
        rows = self._run(
            "MATCH (t:AtomicTask {project: $project}) "
            "DETACH DELETE t "
            "RETURN count(t) AS c",
            {"project": project},
        )
        return int(rows[0]["c"]) if rows else 0

    # -- D: consistency validation ----------------------------------------

    def validate_cycles(self) -> CycleResult:
        rows = self._run(
            "MATCH p=(t:AtomicTask)-[:DEPENDS_ON*2..]->(t) "
            "RETURN [n IN nodes(p) | coalesce(n.name, n.source_file)] AS names LIMIT 1"
        )
        if rows:
            return CycleResult(has_cycle=True, cycle=rows[0]["names"])
        return CycleResult(has_cycle=False)

    def validate_coverage(self) -> list[CoverageGap]:
        """Find dev tasks with no test artifact and no linked QA task.

        A QA task covers a dev task when they share the same parent_feature
        and file_path, and the QA task's task_type is 'qa'.
        """
        rows = self._run(
            """
            MATCH (t:AtomicTask {task_type: 'dev'})
            OPTIONAL MATCH (t)-[:PRODUCED]->(a:Artifact {kind: 'test'})
            WITH t, count(a) AS test_count
            OPTIONAL MATCH (qa:AtomicTask {task_type: 'qa',
                                           parent_feature: t.parent_feature,
                                           file_path: t.file_path})
            WITH t, test_count, count(qa) AS qa_count
            WHERE test_count = 0 AND qa_count = 0
            RETURN t.name AS name, t.file_path AS path, t.parent_feature AS feature
            """
        )
        return [
            CoverageGap(task_name=r["name"], file_path=r["path"], parent_feature=r["feature"])
            for r in rows
        ]

    def validate_api_contracts(self) -> list[ContractViolation]:
        """Find paired FE/BE tasks that disagree on a shared API spec.

        Tasks are paired when they share the same parent_feature and one is
        layer='Frontend' and the other is layer='API' or 'Backend'. They
        violate the contract when their ApiSpec endpoints match but the
        request/response schemas differ.
        """
        rows = self._run(
            """
            MATCH (fe:AtomicTask {layer: 'Frontend'})-[:OWNS_API]->(s1:ApiSpec)
            MATCH (be:AtomicTask)-[:OWNS_API]->(s2:ApiSpec)
            WHERE be.layer IN ['API', 'Backend']
              AND fe.parent_feature = be.parent_feature
              AND s1.endpoint = s2.endpoint
              AND (s1.request_schema <> s2.request_schema
                   OR s1.response_schema <> s2.response_schema)
            RETURN fe.name AS fe_name, be.name AS be_name,
                   s1.endpoint AS endpoint, fe.parent_feature AS feature
            """
        )
        return [
            ContractViolation(
                task_a=r["fe_name"], task_b=r["be_name"],
                endpoint=r["endpoint"], parent_feature=r["feature"],
                reason="mismatched request/response schema",
            )
            for r in rows
        ]
