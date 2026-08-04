"""cie's HTTP surface — mounted into the main be-v2 app (app/api.py),
not run as a separate FastAPI process.

This used to be cie/server.py's own ``FastAPI()`` app, started with
``uvicorn cie.server:app``. That's gone: cie is a library like
any other feature now. ``router`` here is included directly into the app's
single FastAPI instance, so it runs in the same process, on the same port,
started by the same ``uvicorn`` command as everything else.

Two surfaces, unchanged from the old server:

* ``POST /tools/{tool}`` — the unified tool endpoint. The JSON request body
  is the tool's kwargs; the response is ALWAYS the SPEC §0 envelope (see
  :mod:`cie.envelope`), shape-identical to the CLI's ``--json`` mode.
* Legacy REST routes (``/code/*``, ``/tasks/*``, ``/validate/*``,
  ``/query/*``), kept for backwards compatibility.

Multi-project note: the standalone server assumed one project per process
(``CIE_PROJECT``/``FORGE_PROJECT_ID`` read once at first use). Mounted
in the integrated app, one process serves every project, so every route
accepts an optional ``project`` param (query param on GET/PUT routes, a
``project`` key in the JSON body for ``/tools/{tool}`` and the POST/PUT
task routes) and falls back to the env var only when the caller doesn't
supply one. ``cie.factory`` caches one engine/task-repo pair per
project namespace so this doesn't reopen a Neo4j session per request.

Isolation note (v0): the ``run`` tool executes shell commands in a
subprocess with a hard timeout and a cwd jail. The jail root is the env var
``CIE_RUN_ROOT`` (default: the process's current working directory).
There is no container/VM isolation yet — run the whole app inside a sandbox
when executing untrusted generated code.
"""

from __future__ import annotations

import dataclasses
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

from fastapi import APIRouter, HTTPException, Query, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel

from cie import factory
from cie.envelope import ToolTimer, envelope, err_envelope
from cie.models import CoverageSnapshot, TraversalMode
from cie.serialize import edge_record_to_dict, node_to_dict, task_to_dict
from cie.tasks import (
    SCHEMA_VERSION,
    Artifact,
    ArtifactKind,
    AtomicTaskBatch,
    HierarchyNode,
    RepairEvent,
    TaskStatus,
)
from cie.tools import ToolService

router = APIRouter(tags=["cie"])

_tool_services: dict[str, ToolService] = {}


def _project_from_env() -> str:
    """Default project namespace when a request doesn't name one."""
    return os.environ.get("CIE_PROJECT") or os.environ.get(
        "FORGE_PROJECT_ID", ""
    )


def _resolve_project(explicit: str = "") -> str:
    return explicit or _project_from_env()


def get_tool_service(project: str = "") -> ToolService:
    """ToolService for one project namespace, cached (§ module docstring).

    The ``run`` tool's jail root is ``CIE_RUN_ROOT`` (default: cwd).
    """
    project = _resolve_project(project)
    if project not in _tool_services:
        root = Path.cwd()
        allowed_root = Path(os.environ.get("CIE_RUN_ROOT") or str(root))
        _tool_services[project] = factory.build_tool_service(
            project, root=root, allowed_root=allowed_root
        )
    return _tool_services[project]


# ---------------------------------------------------------------------------
# Pydantic request schemas (mirror the planner's schema for the HTTP wire)
# ---------------------------------------------------------------------------
#
# ApiSpecModel/TestTriadModel/AtomicTaskModel/AtomicTaskBatchModel/
# ArtifactModel/RepairEventModel used to duplicate cie.tasks.* as
# separate wire-shape classes. Deleted (core-boundaries-migration-plan.md
# §A.6) — the request bodies below now validate directly against the
# canonical cie.tasks.* types imported above, no second hand-
# maintained copy. StatusUpdateModel stays: it's a genuinely wire-only
# shape (status/updated_at), not a duplicate of any cie.tasks type.


class StatusUpdateModel(BaseModel):
    status: str
    updated_at: str = ""


# ---------------------------------------------------------------------------
# Health plumbing (T3.4) — shared shape with the CLI health command
# ---------------------------------------------------------------------------


def _health_payload(project: str = "") -> dict:
    """Extended health dict: connectivity + counts + tool-layer plumbing."""
    project = _resolve_project(project)
    payload = {
        "status": "ok",
        "store": "reachable",
        "nodes": 0,
        "edges": 0,
        "communities": 0,
        "isolation": "subprocess",
        "schema_version": SCHEMA_VERSION,
        "project": project,
    }
    try:
        engine = factory.get_engine(project)
        health_details = getattr(engine, "health_details", None)
        if callable(health_details):
            details = health_details()
        else:
            # Tolerate engines that only expose stats() (legacy stubs).
            s = engine.stats()
            details = {
                "store": "reachable",
                "nodes": s.nodes,
                "edges": s.edges,
                "communities": s.communities,
                "project": project,
            }
    except Exception as exc:  # noqa: BLE001 - connectivity failure
        payload["status"] = "error"
        payload["store"] = str(exc)
        return payload
    payload.update(
        {
            "store": details.get("store", "reachable"),
            "nodes": details.get("nodes", 0),
            "edges": details.get("edges", 0),
            "communities": details.get("communities", 0),
            "project": details.get("project", project),
        }
    )
    try:
        payload["schema_version"] = factory.get_task_repo(project).schema_version()
    except Exception:  # noqa: BLE001 - version constant is the fallback
        pass
    return payload


@router.get("/health")
def health(project: str = Query("")) -> dict:
    """Graph store connectivity + counts + isolation level + schema version."""
    return _health_payload(project)


@router.get("/tools")
def list_tools(project: str = Query("")) -> dict:
    """Runtime tool-discovery manifest (docs/forge-rebuild-plan.md's
    Phase 2 / WS3, closes review finding P1.6): every callable tool this
    ToolService instance actually has, introspected live via
    `ToolService.describe()` — not a hand-maintained list that can drift
    from what `POST /tools/{tool}` will actually accept. Callers (forge's
    scaffold phase) call this ONCE at run initialization and cache the
    result; see `forge/tools.py::ToolBackend.describe()` and
    `forge/agent.py`'s module docstring."""
    return get_tool_service(project).describe()


@router.get("/schema-version")
def schema_version(project: str = Query("")) -> dict:
    """The atomic-task JSON Schema version enforced at ingest (T3.4)."""
    try:
        version = factory.get_task_repo(_resolve_project(project)).schema_version()
    except Exception:  # noqa: BLE001 - the constant is always available
        version = SCHEMA_VERSION
    return {"schema_version": version}


# ---------------------------------------------------------------------------
# Unified tool endpoint: POST /tools/{tool} (SPEC §6.2)
# ---------------------------------------------------------------------------


def _service_tool(method_name: str) -> Callable[[dict, str], dict]:
    """Adapt a ToolService method (already envelope-returning) to a handler."""

    def handler(kwargs: dict, project: str) -> dict:
        return getattr(get_tool_service(project), method_name)(**kwargs)

    return handler


def _tool_push_tasks(kwargs: dict, project: str) -> dict:
    """push_tasks: validate + partial-accept a batch (T3.1)."""
    tool = "push_tasks"
    with ToolTimer() as timer:
        try:
            batch = AtomicTaskBatch.model_validate(kwargs)
        except ValueError as exc:
            return err_envelope(
                tool, "validation", str(exc),
                hint="validate the batch against schemas/atomic_task.schema.json",
                elapsed_ms=timer.elapsed_ms,
            )
        result = factory.get_task_repo(project).push_tasks(batch, project=project)
    hint = None
    if result.rejected:
        hint = (
            f"{len(result.rejected)} task(s) rejected; fix the reasons and "
            "re-push (push is idempotent per task name)"
        )
    return envelope(
        tool, result.model_dump(mode="json"), hint=hint,
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_set_task_status(kwargs: dict, project: str) -> dict:
    """set_task_status: lifecycle write-back (T3.2)."""
    tool = "set_task_status"
    name = str(kwargs.get("name", ""))
    with ToolTimer() as timer:
        try:
            status = TaskStatus(str(kwargs.get("status", "")))
        except ValueError:
            return err_envelope(
                tool, "validation",
                f"invalid status '{kwargs.get('status')}'",
                hint=f"valid statuses: {[s.value for s in TaskStatus]}",
                elapsed_ms=timer.elapsed_ms,
            )
        updated_at = str(
            kwargs.get("updated_at")
            or datetime.now(timezone.utc).isoformat()
        )
        updated = factory.get_task_repo(project).set_status(name, status, updated_at)
        if not updated:
            return err_envelope(
                tool, "not_found", f"task '{name}' not found",
                hint="call list_pending_tasks to see available tasks",
                elapsed_ms=timer.elapsed_ms,
            )
    return envelope(
        tool,
        {"name": name, "status": status.value, "updated_at": updated_at},
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_link_artifact(kwargs: dict, project: str) -> dict:
    """link_artifact: PRODUCED edge task -> artifact (T3.2)."""
    tool = "link_artifact"
    task_name = str(kwargs.get("task_name", ""))
    with ToolTimer() as timer:
        try:
            kind = ArtifactKind(str(kwargs.get("kind", "source")))
        except ValueError:
            return err_envelope(
                tool, "validation", f"invalid kind '{kwargs.get('kind')}'",
                hint=f"valid kinds: {[k.value for k in ArtifactKind]}",
                elapsed_ms=timer.elapsed_ms,
            )
        artifact = Artifact(
            path=str(kwargs.get("path", "")),
            kind=kind,
            commit_sha=str(kwargs.get("commit_sha", "")),
        )
        linked = factory.get_task_repo(project).add_artifact(task_name, artifact)
        if not linked:
            return err_envelope(
                tool, "not_found", f"task '{task_name}' not found",
                hint="call list_pending_tasks to see available tasks",
                elapsed_ms=timer.elapsed_ms,
            )
    return envelope(
        tool,
        {"task": task_name, "path": artifact.path, "kind": kind.value},
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_append_repair_events(kwargs: dict, project: str) -> dict:
    """append_repair_events: trajectory.jsonl rows -> HAS_EVENT edges (T3.2)."""
    tool = "append_repair_events"
    task_name = str(kwargs.get("task_name", ""))
    raw_events = kwargs.get("events") or []
    with ToolTimer() as timer:
        try:
            events = [RepairEvent.model_validate(e) for e in raw_events]
        except ValueError as exc:
            return err_envelope(
                tool, "validation", str(exc),
                hint="events must match the RepairEvent shape: {round, "
                     "failures_before, failures_after, mode, lint_ok, timestamp}",
                elapsed_ms=timer.elapsed_ms,
            )
        repo = factory.get_task_repo(project)
        for event in events:
            if not repo.add_event(task_name, event):
                return err_envelope(
                    tool, "not_found", f"task '{task_name}' not found",
                    hint="call list_pending_tasks to see available tasks",
                    elapsed_ms=timer.elapsed_ms,
                )
    return envelope(
        tool, {"task": task_name, "events_appended": len(events)},
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_validate_api_contracts(kwargs: dict, project: str) -> dict:
    """validate_api_contracts: FE/BE pairs with divergent ApiSpecs (T3.3)."""
    tool = "validate_api_contracts"
    with ToolTimer() as timer:
        violations = factory.get_task_repo(project).validate_api_contracts()
    return envelope(
        tool, [v.model_dump(mode="json") for v in violations],
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_validate_coverage(kwargs: dict, project: str) -> dict:
    """validate_coverage: dev tasks with no test artifact / QA pair (T3.3)."""
    tool = "validate_coverage"
    with ToolTimer() as timer:
        gaps = factory.get_task_repo(project).validate_coverage()
    hint = (
        "link a QA task or produce a test artifact for each gap"
        if gaps
        else "all dev tasks have a linked QA task or a test artifact"
    )
    return envelope(
        tool, [g.model_dump(mode="json") for g in gaps], hint=hint,
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_record_coverage(kwargs: dict, project: str) -> dict:
    """record_coverage: write one file's coverage measurement + derived
    per-function breakdown (be-v2/docs/design/qa-persona-cie-knowledge-graph.md).
    Custom handler, not a ToolService method — this is a QA-workflow tool,
    not something forge's own repair/generate agent needs, so it stays
    off ToolService.describe()'s manifest (see CieBackend's zero-drift
    test in tests/forge/test_cie_backend_tool_parity.py for why that
    distinction matters)."""
    tool = "record_coverage"
    file_path = str(kwargs.get("file_path", ""))
    with ToolTimer() as timer:
        if not file_path:
            return err_envelope(
                tool, "validation", "file_path is required",
                hint="pass the same repo-relative path the coverage tool reported",
                elapsed_ms=timer.elapsed_ms,
            )
        try:
            coverage_pct = float(kwargs.get("coverage_pct", 0))
            covered_lines = int(kwargs.get("covered_lines", 0))
        except (TypeError, ValueError) as exc:
            return err_envelope(
                tool, "validation",
                f"coverage_pct/covered_lines must be numeric: {exc}",
                elapsed_ms=timer.elapsed_ms,
            )
        uncovered_lines = kwargs.get("uncovered_lines") or []
        measured_at = str(
            kwargs.get("measured_at") or datetime.now(timezone.utc).isoformat()
        )
        subtree = str(kwargs.get("subtree", ""))
        result = factory.get_engine(project).record_coverage(
            file_path, subtree, coverage_pct, covered_lines, uncovered_lines,
            measured_at,
        )
    if result is None:
        return err_envelope(
            tool, "not_found", f"no indexed FILE node for '{file_path}'",
            hint="index the file first (cie load/reindex-file) before "
                 "recording coverage",
            elapsed_ms=timer.elapsed_ms,
        )
    return envelope(tool, dataclasses.asdict(result), elapsed_ms=timer.elapsed_ms)


def _tool_get_coverage(kwargs: dict, project: str) -> dict:
    """get_coverage: one file's latest coverage + per-function breakdown."""
    tool = "get_coverage"
    file_path = str(kwargs.get("file_path", ""))
    subtree = str(kwargs.get("subtree", ""))
    with ToolTimer() as timer:
        if not file_path:
            return err_envelope(
                tool, "validation", "file_path is required",
                elapsed_ms=timer.elapsed_ms,
            )
        result = factory.get_engine(project).get_coverage(file_path, subtree)
    if result is None:
        return err_envelope(
            tool, "not_found", f"'{file_path}' has never been measured",
            hint="call record_coverage first, or check the path with "
                 "file_skeleton",
            elapsed_ms=timer.elapsed_ms,
        )
    return envelope(tool, dataclasses.asdict(result), elapsed_ms=timer.elapsed_ms)


def _tool_coverage_report(kwargs: dict, project: str) -> dict:
    """coverage_report: every file's coverage, worst-first, optional
    below_pct/file_glob/subtree filters. Unmeasured files (never had a
    coverage run) sort first and are included unless
    include_unmeasured=false — "never tested" is itself an answer to
    "what needs better coverage", not something to hide."""
    tool = "coverage_report"
    with ToolTimer() as timer:
        raw_below = kwargs.get("below_pct")
        try:
            below_pct = float(raw_below) if raw_below is not None else None
        except (TypeError, ValueError) as exc:
            return err_envelope(
                tool, "validation", f"below_pct must be numeric: {exc}",
                elapsed_ms=timer.elapsed_ms,
            )
        results = factory.get_engine(project).coverage_report(
            subtree=str(kwargs.get("subtree", "")),
            below_pct=below_pct,
            file_glob=str(kwargs.get("file_glob", "")),
            include_unmeasured=bool(kwargs.get("include_unmeasured", True)),
        )
    hint = (
        "no FILE nodes matched the filters; check subtree/file_glob or "
        "index the project first"
        if not results else None
    )
    return envelope(
        tool, [dataclasses.asdict(r) for r in results], hint=hint,
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_record_coverage_snapshot(kwargs: dict, project: str) -> dict:
    """record_coverage_snapshot: append one aggregate measurement for
    trend history. Always a CREATE (see CoverageSnapshot's docstring) —
    never overwrites a prior snapshot."""
    tool = "record_coverage_snapshot"
    with ToolTimer() as timer:
        try:
            snapshot = CoverageSnapshot(
                project=project,
                subtree=str(kwargs.get("subtree", "")),
                aggregate_pct=float(kwargs.get("aggregate_pct", 0)),
                files_measured=int(kwargs.get("files_measured", 0)),
                total_lines=int(kwargs.get("total_lines", 0)),
                covered_lines=int(kwargs.get("covered_lines", 0)),
                measured_at=str(
                    kwargs.get("measured_at")
                    or datetime.now(timezone.utc).isoformat()
                ),
            )
        except (TypeError, ValueError) as exc:
            return err_envelope(
                tool, "validation", str(exc),
                hint="aggregate_pct/files_measured/total_lines/covered_lines "
                     "must be numeric",
                elapsed_ms=timer.elapsed_ms,
            )
        factory.get_engine(project).record_coverage_snapshot(snapshot)
    return envelope(tool, dataclasses.asdict(snapshot), elapsed_ms=timer.elapsed_ms)


def _tool_coverage_trend(kwargs: dict, project: str) -> dict:
    """coverage_trend: most recent CoverageSnapshots, most recent first."""
    tool = "coverage_trend"
    with ToolTimer() as timer:
        try:
            limit = int(kwargs.get("limit", 20))
        except (TypeError, ValueError) as exc:
            return err_envelope(
                tool, "validation", f"limit must be an integer: {exc}",
                elapsed_ms=timer.elapsed_ms,
            )
        results = factory.get_engine(project).coverage_trend(
            subtree=str(kwargs.get("subtree", "")), limit=limit,
        )
    hint = (
        "no snapshots recorded yet; call record_coverage_snapshot after a "
        "coverage run"
        if not results else None
    )
    return envelope(
        tool, [dataclasses.asdict(r) for r in results], hint=hint,
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_validate_cycles(kwargs: dict, project: str) -> dict:
    """validate_cycles: dependency cycles in the stored task graph (T3.3)."""
    tool = "validate_cycles"
    with ToolTimer() as timer:
        result = factory.get_task_repo(project).validate_cycles()
    hint = (
        "break the cycle or re-push the involved tasks; inspect with "
        "task_dependency_closure"
        if result.has_cycle
        else None
    )
    return envelope(
        tool, result.model_dump(mode="json"), hint=hint,
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_push_hierarchy(kwargs: dict, project: str) -> dict:
    """push_hierarchy: ingest a planner Module->...->UserStory tree (§5.3)."""
    tool = "push_hierarchy"
    tree = {k: v for k, v in kwargs.items() if k != "project"}
    with ToolTimer() as timer:
        try:
            root = HierarchyNode.model_validate(tree)
        except ValueError as exc:
            return err_envelope(
                tool, "validation", str(exc),
                hint="expected a HierarchyNode tree: {node_type, name, id?, "
                     "description?, metadata?, children?}",
                elapsed_ms=timer.elapsed_ms,
            )
        try:
            written = factory.get_hierarchy_repo(project).push_hierarchy(
                root, project=project
            )
        except ValueError as exc:
            return err_envelope(
                tool, "validation", str(exc),
                hint="hierarchy ids must be unique on every root-to-leaf path",
                elapsed_ms=timer.elapsed_ms,
            )
    return envelope(
        tool,
        {
            "nodes_written": written,
            "root_id": root.id,
            "root_name": root.name,
            "project": project,
        },
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_get_children(kwargs: dict, project: str) -> dict:
    """get_children: BFS 'all children' traversal of the PRD hierarchy."""
    tool = "get_children"
    node_id = str(kwargs.get("node_id", ""))
    with ToolTimer() as timer:
        try:
            subtree = factory.get_hierarchy_repo(project).get_children(
                node_id,
                depth=int(kwargs.get("depth", 0)),
                type_filter=str(kwargs.get("type_filter", "")),
                limit=int(kwargs.get("limit", 200)),
            )
        except ValueError:
            return err_envelope(
                tool, "not_found", f"unknown hierarchy node '{node_id}'",
                hint="push the hierarchy first with push_hierarchy",
                elapsed_ms=timer.elapsed_ms,
            )
    hint = (
        "children capped at the limit; narrow with depth or type_filter"
        if subtree.truncated
        else None
    )
    return envelope(
        tool,
        {
            "root": subtree.root.model_dump(mode="json"),
            "children": [c.model_dump(mode="json") for c in subtree.children],
        },
        truncated=subtree.truncated, hint=hint, elapsed_ms=timer.elapsed_ms,
    )


def _tool_get_lineage(kwargs: dict, project: str) -> dict:
    """get_lineage: ancestor path of a hierarchy node, root-first."""
    tool = "get_lineage"
    node_id = str(kwargs.get("node_id", ""))
    with ToolTimer() as timer:
        lineage = factory.get_hierarchy_repo(project).get_lineage(node_id)
    hint = None if lineage else (
        f"unknown hierarchy node '{node_id}'; push the hierarchy first with "
        "push_hierarchy"
    )
    return envelope(
        tool, [v.model_dump(mode="json") for v in lineage], hint=hint,
        elapsed_ms=timer.elapsed_ms,
    )


def _tool_health(kwargs: dict, project: str) -> dict:
    """health: connectivity + counts + isolation + schema version (T3.4)."""
    tool = "health"
    with ToolTimer() as timer:
        payload = _health_payload(project)
    hint = (
        None
        if payload["status"] == "ok"
        else "check the graph store configuration and that it is running"
    )
    return envelope(tool, payload, hint=hint, elapsed_ms=timer.elapsed_ms)


def _tool_schema_version(kwargs: dict, project: str) -> dict:
    """schema_version: the atomic-task schema version enforced at ingest."""
    tool = "schema_version"
    with ToolTimer() as timer:
        try:
            version = factory.get_task_repo(project).schema_version()
        except Exception:  # noqa: BLE001 - the constant is always available
            version = SCHEMA_VERSION
    return envelope(
        tool, {"schema_version": version}, elapsed_ms=timer.elapsed_ms
    )


#: tool name -> handler(kwargs, project) -> SPEC §0 envelope dict.
TOOLS: dict[str, Callable[[dict, str], dict]] = {
    # Tier 1
    "view_file": _service_tool("view_file"),
    "search_symbol": _service_tool("search_symbol"),
    "semantic_search": _service_tool("semantic_search"),
    "callers": _service_tool("callers"),
    "callees": _service_tool("callees"),
    "run": _service_tool("run"),
    # Tier 2
    "file_skeleton": _service_tool("file_skeleton"),
    "path_between": _service_tool("path_between"),
    "failing_context": _service_tool("failing_context"),
    "affected_by": _service_tool("affected_by"),
    "blame_history": _service_tool("blame_history"),
    # Index lifecycle
    "reindex_file": _service_tool("reindex_file"),
    # -- write tools --
    "write_file": _service_tool("write_file"),
    "edit_file": _service_tool("edit_file"),
    "delete_file": _service_tool("delete_file"),
    # Tier 3: task supply + write-back + validation
    "list_pending_tasks": _service_tool("list_pending_tasks"),
    "push_tasks": _tool_push_tasks,
    "get_task": _service_tool("get_task"),
    "task_dependency_closure": _service_tool("task_dependency_closure"),
    "set_task_status": _tool_set_task_status,
    "link_artifact": _tool_link_artifact,
    "append_repair_events": _tool_append_repair_events,
    "validate_api_contracts": _tool_validate_api_contracts,
    "validate_coverage": _tool_validate_coverage,
    "validate_cycles": _tool_validate_cycles,
    # PRD hierarchy
    "push_hierarchy": _tool_push_hierarchy,
    "get_children": _tool_get_children,
    "get_lineage": _tool_get_lineage,
    # QA coverage (be-v2/docs/design/qa-persona-cie-knowledge-graph.md)
    "record_coverage": _tool_record_coverage,
    "get_coverage": _tool_get_coverage,
    "coverage_report": _tool_coverage_report,
    "record_coverage_snapshot": _tool_record_coverage_snapshot,
    "coverage_trend": _tool_coverage_trend,
    # Operational plumbing
    "health": _tool_health,
    "schema_version": _tool_schema_version,
}

#: error kind -> HTTP status for failed tool envelopes.
_ERROR_STATUS = {"not_found": 404, "validation": 422, "internal": 500}


@router.post("/tools/{tool}")
async def run_tool(tool: str, request: Request) -> JSONResponse:
    """Unified tool endpoint: kwargs as JSON body, ALWAYS the SPEC §0 envelope.

    Isolation (v0): the ``run`` tool spawns a subprocess jailed to
    ``CIE_RUN_ROOT`` (default: process cwd) with a hard timeout and
    head+tail output truncation — there is no container isolation yet.
    """
    handler = TOOLS.get(tool)
    if handler is None:
        return JSONResponse(
            status_code=404,
            content=err_envelope(
                tool, "not_found", f"unknown tool '{tool}'",
                hint=f"valid tools: {', '.join(sorted(TOOLS))}",
            ),
        )
    try:
        body = await request.json()
    except Exception:  # noqa: BLE001 - malformed JSON body
        return JSONResponse(
            status_code=422,
            content=err_envelope(
                tool, "validation", "request body must be a JSON object",
                hint=f"POST the tool's kwargs as a JSON object, e.g. "
                     f"'{{\"name\": \"parse_csv\"}}' for search_symbol",
            ),
        )
    if body is None:
        body = {}
    if not isinstance(body, dict):
        return JSONResponse(
            status_code=422,
            content=err_envelope(
                tool, "validation", "request body must be a JSON object",
                hint=f"POST the tool's kwargs as a JSON object, e.g. "
                     f"'{{\"name\": \"parse_csv\"}}' for search_symbol",
            ),
        )
    project = _resolve_project(str(body.pop("project", "") or ""))
    try:
        payload = handler(body, project)
    except TypeError as exc:
        return JSONResponse(
            status_code=422,
            content=err_envelope(
                tool, "validation", f"bad arguments for '{tool}': {exc}",
                hint="check the tool's parameter names in the README tool table",
            ),
        )
    except ValueError as exc:
        return JSONResponse(
            status_code=422,
            content=err_envelope(
                tool, "validation", str(exc),
                hint="check the argument values against the tool contract",
            ),
        )
    except Exception as exc:  # noqa: BLE001 - backend failure
        return JSONResponse(
            status_code=500,
            content=err_envelope(
                tool, "internal", f"{type(exc).__name__}: {exc}",
                hint="unexpected tool failure; check server logs and /health",
            ),
        )
    status = 200
    if not payload.get("ok") and payload.get("error"):
        status = _ERROR_STATUS.get(payload["error"].get("kind"), 500)
    return JSONResponse(status_code=status, content=payload)


# ---------------------------------------------------------------------------
# A: task supply (legacy REST)
# ---------------------------------------------------------------------------


@router.get("/tasks/pending")
def list_pending_tasks(project: str = Query("")) -> list[dict]:
    repo = factory.get_task_repo(_resolve_project(project))
    return [task_to_dict(t) for t in repo.list_pending()]


@router.post("/tasks")
def submit_tasks(batch: AtomicTaskBatch, project: str = Query("")) -> dict:
    repo = factory.get_task_repo(_resolve_project(project))
    try:
        count = repo.submit_batch(batch)
        return {"accepted": count}
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc))


@router.get("/tasks/{name}")
def get_task(name: str, project: str = Query("")) -> dict:
    repo = factory.get_task_repo(_resolve_project(project))
    t = repo.get_task(name)
    if t is None:
        raise HTTPException(status_code=404, detail=f"task '{name}' not found")
    return task_to_dict(t)


@router.get("/tasks/{name}/dependencies")
def get_task_dependencies(name: str, project: str = Query("")) -> list[str]:
    repo = factory.get_task_repo(_resolve_project(project))
    return repo.get_dependencies(name)


# ---------------------------------------------------------------------------
# B: status write-back (legacy REST)
# ---------------------------------------------------------------------------


@router.put("/tasks/{name}/status")
def set_task_status(name: str, update: StatusUpdateModel, project: str = Query("")) -> dict:
    try:
        status = TaskStatus(update.status)
    except ValueError:
        raise HTTPException(status_code=422, detail=f"invalid status '{update.status}'")
    updated_at = update.updated_at or datetime.now(timezone.utc).isoformat()
    repo = factory.get_task_repo(_resolve_project(project))
    if not repo.set_status(name, status, updated_at):
        raise HTTPException(status_code=404, detail=f"task '{name}' not found")
    return {"name": name, "status": status.value, "updated_at": updated_at}


@router.post("/tasks/{name}/artifacts")
def add_artifact(name: str, artifact: Artifact, project: str = Query("")) -> dict:
    repo = factory.get_task_repo(_resolve_project(project))
    if not repo.add_artifact(name, artifact):
        raise HTTPException(status_code=404, detail=f"task '{name}' not found")
    return {"task": name, "path": artifact.path, "kind": artifact.kind.value}


@router.post("/tasks/{name}/events")
def add_event(name: str, event: RepairEvent, project: str = Query("")) -> dict:
    repo = factory.get_task_repo(_resolve_project(project))
    if not repo.add_event(name, event):
        raise HTTPException(status_code=404, detail=f"task '{name}' not found")
    return {"task": name, "round": event.round}


# ---------------------------------------------------------------------------
# C: code-structure queries (legacy REST)
# ---------------------------------------------------------------------------


@router.get("/code/symbol")
def code_symbol(name: str = Query(...), project: str = Query("")) -> dict:
    engine = factory.get_engine(_resolve_project(project))
    sig = engine.get_signature(name)
    if sig is None:
        nr = engine.get_node(name)
        if nr is None:
            raise HTTPException(status_code=404, detail=f"symbol '{name}' not found")
        return {"node": node_to_dict(nr.node), "degree": nr.degree, "signature": None}
    from cie.serialize import signature_to_dict

    return signature_to_dict(sig)


@router.get("/code/callers")
def code_callers(symbol: str = Query(...), project: str = Query("")) -> list[dict]:
    """Callers of a symbol as EdgeRecord dicts (B1 contract).

    ``engine.get_callers`` returns ``list[EdgeRecord]`` (not Nodes), so the
    payload is edge-shaped: caller/callee labels plus relation + confidence.
    """
    engine = factory.get_engine(_resolve_project(project))
    return [
        {
            "caller": rec.source_label,
            "callee": rec.target_label,
            **edge_record_to_dict(rec),
        }
        for rec in engine.get_callers(symbol)
    ]


@router.get("/code/file")
def code_file(path: str = Query(...), project: str = Query("")) -> list[dict]:
    """All symbols defined in a file (nodes ordered by line_start)."""
    engine = factory.get_engine(_resolve_project(project))
    return [node_to_dict(n) for n in engine.get_file_skeleton(path)]


@router.get("/code/path")
def code_path(
    source: str = Query(...),
    target: str = Query(...),
    max_hops: int = Query(8, ge=1, le=20),
    project: str = Query(""),
) -> dict:
    engine = factory.get_engine(_resolve_project(project))
    result = engine.shortest_path(source, target, max_hops)
    if result is None:
        raise HTTPException(status_code=404, detail="no path found")
    return {
        "hops": result.hops,
        "nodes": [node_to_dict(n) for n in result.nodes],
        "edges": [
            {
                "source": e.edge.source,
                "target": e.edge.target,
                "relation": e.edge.relation,
                "confidence": e.edge.confidence.value,
            }
            for e in result.edges
        ],
    }


@router.post("/code/reload")
def code_reload(path: str = Query(...), project: str = Query("")) -> dict:
    from cie.extract import extract_tree

    p = Path(path)
    if not p.exists():
        raise HTTPException(status_code=404, detail=f"path '{path}' not found")
    extraction = extract_tree(p)
    engine = factory.get_engine(_resolve_project(project))
    count = engine._repo.load_extraction(extraction.nodes, extraction.edges)  # noqa: SLF001
    return {"loaded_nodes": count, "loaded_edges": len(extraction.edges)}


# ---------------------------------------------------------------------------
# D: consistency validation (legacy REST)
# ---------------------------------------------------------------------------


@router.get("/validate/cycles")
def validate_cycles(project: str = Query("")) -> dict:
    repo = factory.get_task_repo(_resolve_project(project))
    result = repo.validate_cycles()
    return {"has_cycle": result.has_cycle, "cycle": list(result.cycle)}


@router.get("/validate/coverage")
def validate_coverage(project: str = Query("")) -> list[dict]:
    repo = factory.get_task_repo(_resolve_project(project))
    return [g.model_dump() for g in repo.validate_coverage()]


@router.get("/validate/api-contracts")
def validate_api_contracts(project: str = Query("")) -> list[dict]:
    repo = factory.get_task_repo(_resolve_project(project))
    return [v.model_dump() for v in repo.validate_api_contracts()]


# ---------------------------------------------------------------------------
# E: graph query passthroughs (JSON versions of CLI commands)
# ---------------------------------------------------------------------------


@router.get("/query/search")
def query_search(
    question: str = Query(...),
    mode: str = Query("bfs"),
    depth: int = Query(3, ge=1, le=6),
    project: str = Query(""),
) -> dict:
    engine = factory.get_engine(_resolve_project(project))
    result = engine.search(question, TraversalMode(mode), depth)
    return {
        "mode": result.mode.value,
        "depth": result.depth,
        "start_labels": list(result.start_labels),
        "nodes": [node_to_dict(n) for n in result.nodes],
        "edges": [edge_record_to_dict(e) for e in result.edges],
    }


@router.get("/query/stats")
def query_stats(project: str = Query("")) -> dict:
    engine = factory.get_engine(_resolve_project(project))
    s = engine.stats()
    return {
        "nodes": s.nodes,
        "edges": s.edges,
        "communities": s.communities,
        "confidence_counts": s.confidence_counts,
        "confidence_percent": s.confidence_percent,
    }


@router.get("/query/god")
def query_god(top_n: int = Query(10, ge=1, le=100), project: str = Query("")) -> list[dict]:
    engine = factory.get_engine(_resolve_project(project))
    return [
        {"label": nr.node.label, "id": nr.node.id, "degree": nr.degree}
        for nr in engine.god_nodes(top_n)
    ]


@router.get("/query/discover")
def query_discover(project: str = Query("")) -> list[str]:
    engine = factory.get_engine(_resolve_project(project))
    return engine.discover_features()


# ---------------------------------------------------------------------------
# PRD hierarchy REST routes — real GET endpoints over hierarchy.py's
# Neo4jHierarchyRepository (previously only reachable via the generic
# POST /tools/{tool} dispatcher's get_children/get_lineage handlers above).
# ---------------------------------------------------------------------------


@router.get("/hierarchy/{node_id}")
def get_hierarchy_node_route(node_id: str, project: str = Query("")) -> dict:
    repo = factory.get_hierarchy_repo(_resolve_project(project))
    node = repo.get_hierarchy_node(node_id)
    if node is None:
        raise HTTPException(status_code=404, detail=f"Node {node_id} not found")
    # get_hierarchy_node itself never resolves these (see its docstring) —
    # stamp them here by reusing get_lineage/get_children rather than a
    # third traversal implementation.
    lineage = repo.get_lineage(node_id)
    node.parent_id = lineage[-2].id if len(lineage) >= 2 else None
    node.children_ids = [c.id for c in repo.get_children(node_id, depth=1).children]
    return node.model_dump()


@router.get("/hierarchy/{node_id}/relations")
def get_hierarchy_relations(node_id: str, project: str = Query("")) -> dict:
    repo = factory.get_hierarchy_repo(_resolve_project(project))
    lineage = repo.get_lineage(node_id)  # root-first, node itself last
    if not lineage:
        raise HTTPException(status_code=404, detail=f"Node {node_id} not found")
    children = repo.get_children(node_id, depth=1)
    return {
        "id": node_id,
        "parent": lineage[-2].model_dump() if len(lineage) >= 2 else None,
        "ancestors": [n.model_dump() for n in lineage[:-1]],
        "children": [c.model_dump() for c in children.children],
    }


@router.get("/hierarchy/project/{project_id}")
def get_project_hierarchy(project_id: str) -> list[dict]:
    repo = factory.get_hierarchy_repo(project_id)
    return [n.model_dump() for n in repo.get_project_tree(project_id)]


# ---------------------------------------------------------------------------
# Whole-project code-structure graph (AST/code-graph, distinct from the
# PRD/business hierarchy above) — was entirely missing under be-v2-alone
# (docs/frontend-bev2-api-audit-solution.md Tier A; plan: docs/frontend-
# bev2-sync-execution-plan.md Phase 1.3). Data source: forge already keeps
# this graph in sync as it writes generated files (ToolService's
# _sync_graph_after_write, forge/tools.py), scoped to the SAME `project`
# string build_app/forge_repair thread explicitly into make_tools(project=)
# — confirmed no env-var-fallback collision risk before writing this route
# (Phase 1.3 precheck). Called by frontend/src/features/extractor/api/
# extractorApi.ts::fetchProjectCodeGraph.
# ---------------------------------------------------------------------------


@router.get("/api/projects/{project_id}/code-graph")
def get_project_code_graph(project_id: str) -> dict:
    engine = factory.get_engine(project_id)
    nodes, edges = engine.project_graph()
    return {
        "nodes": [node_to_dict(n) for n in nodes],
        "edges": [
            {
                "source": e.edge.source,
                "target": e.edge.target,
                "relation": e.edge.relation,
                "confidence": e.edge.confidence.value,
            }
            for e in edges
        ],
    }
