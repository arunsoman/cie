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
import threading
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

from fastapi import APIRouter, Depends, HTTPException, Query, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

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
from cie.tool_policy import (
    INSPECTOR_POLICY,
    ORCHESTRATOR_POLICY,
    REQUIREMENT_MINER_POLICY,
    WRITE_TOOLS,
    ToolNotPermitted,
    ToolPolicy,
    authorize,
)
from cie.tools import ToolService

router = APIRouter(tags=["cie"])

_tool_services: dict[str, ToolService] = {}
#: SF5: guards _tool_services' check-then-act construction below. A
#: per-project lock (not one global lock) so two concurrent FIRST
#: requests for DIFFERENT never-cached projects don't serialize behind
#: each other — only requests racing for the SAME project contend.
#: _tool_services_locks_guard protects the lock dict itself (a plain
#: dict, unlike Python's GIL-protected simple get/set, isn't safe against
#: two threads both creating a NEW per-project lock for the same missing
#: key at once).
_tool_services_locks: dict[str, threading.Lock] = {}
_tool_services_locks_guard = threading.Lock()


def _project_lock(project: str) -> threading.Lock:
    with _tool_services_locks_guard:
        lock = _tool_services_locks.get(project)
        if lock is None:
            lock = threading.Lock()
            _tool_services_locks[project] = lock
        return lock


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

    SF5: construction is guarded by a per-project lock (double-checked —
    the fast path for an already-cached project takes no lock at all).
    Previously this was a bare check-then-act: two concurrent first
    requests for the same never-cached project could both see the miss,
    both build a ToolService/engine pair, and the second assignment
    silently won, wasting the first construction's resources — not
    data-corrupting, but real resource duplication on the ~90-tool hot
    path.
    """
    project = _resolve_project(project)
    if project not in _tool_services:
        with _project_lock(project):
            if project not in _tool_services:
                root = Path.cwd()
                allowed_root = Path(os.environ.get("CIE_RUN_ROOT") or str(root))
                _tool_services[project] = factory.build_tool_service(
                    project, root=root, allowed_root=allowed_root
                )
    return _tool_services[project]


# ---------------------------------------------------------------------------
# HTTP tool policy — the real adoption of cie.tool_policy at an external
# (less-trusted) caller boundary, which that module's own docstring named
# as its intended follow-up work.
# ---------------------------------------------------------------------------

#: POST /tools/{tool} handlers that mutate the task/hierarchy/coverage
#: repository but are NOT ToolService methods. (Historical note: this was
#: once 17 bespoke `_tool_*` handlers; R1 promoted the six task/QA write-
#: back tools, R14 promoted the three hierarchy tools — all now real
#: ToolService methods whose authorization flows through WRITE_TOOLS.
#: The set is now EMPTY, permanently: a promoted name must move to
#: WRITE_TOOLS in the same commit (pinned by
#: `test_http_write_aliases_and_write_tools_never_overlap`), so a future
#: promotion can't double-classify it.
HTTP_WRITE_ALIASES: frozenset[str] = frozenset()


def _http_policy() -> ToolPolicy:
    """The ToolPolicy governing this HTTP surface.

    Read-only by default (INSPECTOR) — a browser tab or drive-by POST
    must never be able to mutate files or the graph. ``CIE_HTTP_POLICY``
    selects the policy explicitly (inspector | miner | orchestrator);
    ``CIE_HTTP_ALLOW_WRITE=1`` is the shorter escape hatch to
    orchestrator. Mutating requests are additionally subject to the
    cross-origin guard (see `_origin_allowed` / `_write_guard`).
    """
    name = os.environ.get("CIE_HTTP_POLICY", "inspector").strip().lower()
    if os.environ.get("CIE_HTTP_ALLOW_WRITE", "").strip().lower() in {
        "1", "true", "yes", "on",
    }:
        name = "orchestrator"
    return {
        "inspector": INSPECTOR_POLICY,
        "miner": REQUIREMENT_MINER_POLICY,
        "orchestrator": ORCHESTRATOR_POLICY,
    }.get(name, INSPECTOR_POLICY)


def _authorize_http_tool(tool: str) -> ToolPolicy:
    """Policy gate for ONE HTTP tool dispatch — ToolService methods via
    `authorize()` (against WRITE_TOOLS), the bespoke alias handlers via
    the allow_write flag (`HTTP_WRITE_ALIASES` names aren't ToolService
    methods, so WRITE_TOOLS can't cover them). Raises ToolNotPermitted.
    """
    policy = _http_policy()
    if tool in HTTP_WRITE_ALIASES:
        if not policy.allow_write:
            raise ToolNotPermitted(tool, policy)
    else:
        authorize(policy, tool)
    return policy


def _is_write_tool(tool: str) -> bool:
    return tool in WRITE_TOOLS or tool in HTTP_WRITE_ALIASES


def _origin_allowed(request: Request) -> bool:
    """True for non-browser callers (no ``Origin`` header at all) and for
    same-origin browser callers.

    A cross-origin ``Origin`` on a MUTATING request is the
    CSRF-to-localhost vector: a ``text/plain`` POST needs no CORS
    preflight to have side effects server-side (reading the response is
    what CORS blocks, not sending). It must therefore be either the
    request's own Host or listed in ``CIE_HTTP_ALLOWED_ORIGINS``
    (comma-separated)."""
    origin = request.headers.get("origin", "")
    if not origin:
        return True
    host = request.headers.get("host", "")
    if host and origin.rstrip("/") in (f"http://{host}", f"https://{host}"):
        return True
    allowed = {
        o.strip().rstrip("/")
        for o in os.environ.get("CIE_HTTP_ALLOWED_ORIGINS", "").split(",")
        if o.strip()
    }
    return origin.rstrip("/") in allowed


def _write_guard(request: Request) -> None:
    """FastAPI dependency for legacy REST routes that mutate state (task
    push/status/artifact/event, reindex, sync ingestion, telemetry):
    write policy required — the default read-only surface rejects them —
    plus the same cross-origin guard the tool dispatcher applies to its
    write tools."""
    if not _http_policy().allow_write:
        raise HTTPException(
            status_code=403,
            detail=(
                "mutating endpoint: the HTTP surface is read-only by "
                "default (CIE_HTTP_POLICY=inspector); set "
                "CIE_HTTP_POLICY=orchestrator or CIE_HTTP_ALLOW_WRITE=1 "
                "to enable writes"
            ),
        )
    if not _origin_allowed(request):
        raise HTTPException(
            status_code=403,
            detail=(
                "cross-origin write request rejected; add this origin to "
                "CIE_HTTP_ALLOWED_ORIGINS if it is intentional"
            ),
        )


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
    `forge/agent.py`'s module docstring.

    The manifest is filtered through `_http_policy()`: under the default
    read-only policy, write tools (WRITE_TOOLS + HTTP_WRITE_ALIASES) are
    dropped from discovery exactly as `POST /tools/{tool}` itself would
    reject them — a caller can't even see a tool it isn't authorized
    for, same guarantee `filter_tool_schemas` gives the MCP path."""
    policy = _http_policy()
    manifest = get_tool_service(project).describe()
    if not policy.allow_write:
        manifest["results"] = [
            t for t in manifest.get("results", [])
            if not _is_write_tool(str(t.get("name", "")))
        ]
        if "tool_count" in manifest:
            manifest["tool_count"] = len(manifest["results"])
    manifest["http_policy"] = {
        "agent_type": policy.agent_type.value,
        "allow_write": policy.allow_write,
    }
    return manifest


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
    "get_meta": _service_tool("get_meta"),
    "get_function": _service_tool("get_function"),
    "ls": _service_tool("ls"),
    "dir": _service_tool("dir"),
    "file_hierarchy": _service_tool("file_hierarchy"),
    "file_names_like": _service_tool("file_names_like"),
    "path_prefix": _service_tool("path_prefix"),
    "search_symbol": _service_tool("search_symbol"),
    "resolve_import": _service_tool("resolve_import"),
    "semantic_search": _service_tool("semantic_search"),
    "callers": _service_tool("callers"),
    "callees": _service_tool("callees"),
    "run": _service_tool("run"),
    # Tier 2
    "file_skeleton": _service_tool("file_skeleton"),
    "path_between": _service_tool("path_between"),
    "failing_context": _service_tool("failing_context"),
    "affected_by": _service_tool("affected_by"),
    "class_hierarchy": _service_tool("class_hierarchy"),
    "test_map": _service_tool("test_map"),
    "actual_callers": _service_tool("actual_callers"),
    "dead_code_confirm": _service_tool("dead_code_confirm"),
    "hybrid_search": _service_tool("hybrid_search"),
    "entity_context": _service_tool("entity_context"),
    "qa": _service_tool("qa"),
    "blame_history": _service_tool("blame_history"),
    # Section 13 (Code Intelligence)
    "clone_detect_run": _service_tool("clone_detect_run"),
    "clone_clusters": _service_tool("clone_clusters"),
    "clone_find": _service_tool("clone_find"),
    "performance_analyze_run": _service_tool("performance_analyze_run"),
    "performance_profile": _service_tool("performance_profile"),
    "antipattern_scan": _service_tool("antipattern_scan"),
    "drift_detect_run": _service_tool("drift_detect_run"),
    "drift_report": _service_tool("drift_report"),
    "architecture_check": _service_tool("architecture_check"),
    "metrics": _service_tool("metrics"),
    "tech_debt_report": _service_tool("tech_debt_report"),
    "metric_trend": _service_tool("metric_trend"),
    "graph_diff": _service_tool("graph_diff"),
    "community_detect_run": _service_tool("community_detect_run"),
    "community_summarize_run": _service_tool("community_summarize_run"),
    "community_search": _service_tool("community_search"),
    "accuracy_check": _service_tool("accuracy_check"),
    "freshness_report": _service_tool("freshness_report"),
    "comprehensiveness_report": _service_tool("comprehensiveness_report"),
    "salience_report": _service_tool("salience_report"),
    # Section 0 (Population & Real-Time Sync)
    "sync_quality_gate": _service_tool("sync_quality_gate"),
    "sync_promote": _service_tool("sync_promote"),
    "sync_revert": _service_tool("sync_revert"),
    "sync_ast_delta": _service_tool("sync_ast_delta"),
    "sync_evict_speculative": _service_tool("sync_evict_speculative"),
    "sync_load_commit": _service_tool("sync_load_commit"),
    "configure_layer_rules": _service_tool("configure_layer_rules"),
    "get_layer_rules": _service_tool("get_layer_rules"),
    "install_git_hook": _service_tool("install_git_hook"),
    # Section 1 (Core Data Model)
    "export_rdf": _service_tool("export_rdf"),
    "related_edges": _service_tool("related_edges"),
    "validate_property_constraints": _service_tool("validate_property_constraints"),
    "type_flow_run": _service_tool("type_flow_run"),
    "type_flow": _service_tool("type_flow"),
    "dependency_graph_run": _service_tool("dependency_graph_run"),
    "dependency_graph": _service_tool("dependency_graph"),
    "vulnerability_scan_run": _service_tool("vulnerability_scan_run"),
    "vulnerabilities": _service_tool("vulnerabilities"),
    "doc_graph_run": _service_tool("doc_graph_run"),
    "doc_search": _service_tool("doc_search"),
    # Section 14 (Confidence Framework Integration)
    "contracts_run": _service_tool("contracts_run"),
    "contracts": _service_tool("contracts"),
    "validate_types": _service_tool("validate_types"),
    "inject_assertions": _service_tool("inject_assertions"),
    "strip_assertions": _service_tool("strip_assertions"),
    "test_skeletons_run": _service_tool("test_skeletons_run"),
    "test_skeletons": _service_tool("test_skeletons"),
    "test_coverage": _service_tool("test_coverage"),
    "state_machine_run": _service_tool("state_machine_run"),
    "state_machine": _service_tool("state_machine"),
    "fsm_validate": _service_tool("fsm_validate"),
    "traceability_coverage": _service_tool("traceability_coverage"),
    "traceability_orphans": _service_tool("traceability_orphans"),
    "traceability_chain": _service_tool("traceability_chain"),
    "prd_traceability_coverage": _service_tool("prd_traceability_coverage"),
    "prd_traceability_orphans": _service_tool("prd_traceability_orphans"),
    "prd_traceability_chain": _service_tool("prd_traceability_chain"),
    "semantic_diff": _service_tool("semantic_diff"),
    "record_verdict": _service_tool("record_verdict"),
    "agent_verdicts": _service_tool("agent_verdicts"),
    "confidence_report": _service_tool("confidence_report"),
    "justification": _service_tool("justification"),
    "check_invariant": _service_tool("check_invariant"),
    "invariant_violations": _service_tool("invariant_violations"),
    "telemetry_to_spec": _service_tool("telemetry_to_spec"),
    # Section 15 (Decomposition Engine)
    "decompose_page": _service_tool("decompose_page"),
    "page_tree": _service_tool("page_tree"),
    "promote_hint_to_task": _service_tool("promote_hint_to_task"),
    "element_coverage": _service_tool("element_coverage"),
    "implied_pages_run": _service_tool("implied_pages_run"),
    "implied_pages": _service_tool("implied_pages"),
    # Section 17 (System Intelligence & Subsystem Health)
    "subsystem_health": _service_tool("subsystem_health"),
    "subsystem_gaps": _service_tool("subsystem_gaps"),
    "subsystem_dependency_graph": _service_tool("subsystem_dependency_graph"),
    "subsystem_dependency_graph_run": _service_tool("subsystem_dependency_graph_run"),
    "population_path": _service_tool("population_path"),
    # Section 16 (Autonomous Test Execution & APM)
    "test_plan": _service_tool("test_plan"),
    "run_tests": _service_tool("run_tests"),
    "record_test_result": _service_tool("record_test_result"),
    "test_results": _service_tool("test_results"),
    "coverage_gaps": _service_tool("coverage_gaps"),
    "nook_and_corner_test": _service_tool("nook_and_corner_test"),
    "unified_coverage_report": _service_tool("unified_coverage_report"),
    "mock_registry_run": _service_tool("mock_registry_run"),
    "mock_registry": _service_tool("mock_registry"),
    "mock_coverage": _service_tool("mock_coverage"),
    "start_mock_server": _service_tool("start_mock_server"),
    "stop_mock_server": _service_tool("stop_mock_server"),
    "mock_violations": _service_tool("mock_violations"),
    "record_apm_metric": _service_tool("record_apm_metric"),
    "apm_metrics": _service_tool("apm_metrics"),
    "performance_baseline": _service_tool("performance_baseline"),
    "performance_regressions": _service_tool("performance_regressions"),
    # Index lifecycle
    "reindex_file": _service_tool("reindex_file"),
    "reindex": _service_tool("reindex"),
    "start_watch": _service_tool("start_watch"),
    "stop_watch": _service_tool("stop_watch"),
    # -- write tools --
    # Repair transaction layer (propose → apply → verify; see cie.patch).
    # apply_patch is the only file-mutating tool; propose/verify persist
    # their immutable PatchPlan nodes / lifecycle status to the graph only.
    "propose_patch": _service_tool("propose_patch"),
    "apply_patch": _service_tool("apply_patch"),
    "verify_patch": _service_tool("verify_patch"),
    "get_patch": _service_tool("get_patch"),
    "list_patches": _service_tool("list_patches"),
    "write_file": _service_tool("write_file"),
    "write_files_atomic": _service_tool("write_files_atomic"),
    "edit_file": _service_tool("edit_file"),
    "delete_file": _service_tool("delete_file"),
    # Tier 3: task supply + write-back + validation
    "list_pending_tasks": _service_tool("list_pending_tasks"),
    "push_tasks": _service_tool("push_tasks"),
    "get_task": _service_tool("get_task"),
    "task_dependency_closure": _service_tool("task_dependency_closure"),
    "set_task_status": _service_tool("set_task_status"),
    "link_artifact": _service_tool("link_artifact"),
    "append_repair_events": _service_tool("append_repair_events"),
    "validate_api_contracts": _tool_validate_api_contracts,
    "validate_coverage": _tool_validate_coverage,
    "validate_cycles": _tool_validate_cycles,
    # PRD hierarchy
    "push_hierarchy": _service_tool("push_hierarchy"),
    "get_children": _service_tool("get_children"),
    "get_lineage": _service_tool("get_lineage"),
    # QA coverage (be-v2/docs/design/qa-persona-cie-knowledge-graph.md)
    "record_coverage": _service_tool("record_coverage"),
    "get_coverage": _tool_get_coverage,
    "coverage_report": _tool_coverage_report,
    "record_coverage_snapshot": _service_tool("record_coverage_snapshot"),
    "coverage_trend": _tool_coverage_trend,
    # Operational plumbing
    "health": _tool_health,
    "schema_version": _tool_schema_version,
}

#: error kind -> HTTP status for failed tool envelopes.
_ERROR_STATUS = {"not_found": 404, "validation": 422, "unavailable": 503, "forbidden": 403, "internal": 500}


@router.post("/tools/{tool}")
async def run_tool(tool: str, request: Request) -> JSONResponse:
    """Unified tool endpoint: kwargs as JSON body, ALWAYS the SPEC §0 envelope.

    Isolation (v0): the ``run`` tool spawns a subprocess jailed to
    ``CIE_RUN_ROOT`` (default: process cwd) with a hard timeout and
    head+tail output truncation — there is no container isolation yet.

    This is the only `async def` route in this module — every other route
    here is a plain `def`, which Starlette already dispatches to its own
    threadpool automatically, so only this one needed the explicit
    `run_in_threadpool` wrap below. `handler(body, project)` ultimately
    calls into `Neo4jRepository`'s synchronous driver (no async Neo4j
    driver in this codebase — see that class's own docstring for why a
    full async-driver migration isn't a safe fit for this pass); running
    it inline on this `async def`'s own coroutine would block the ASGI
    event loop for every OTHER concurrent request on this worker for the
    duration of the Neo4j call, not just this one.
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
    # Policy gate BEFORE any dispatch (see _http_policy): the HTTP surface
    # is read-only by default, so a write tool here is a server-side 403 —
    # enforced in this surface, not left to the connecting client's
    # settings. This is what cie's per-agent-policy differentiator claim
    # promises; `tool_policy.py` named "an external HTTP caller" as its
    # intended adopter.
    try:
        _authorize_http_tool(tool)
    except ToolNotPermitted as exc:
        return JSONResponse(
            status_code=403,
            content=err_envelope(
                tool, "forbidden", str(exc),
                hint="the HTTP surface is read-only by default; set "
                     "CIE_HTTP_POLICY=orchestrator (or CIE_HTTP_ALLOW_WRITE=1) "
                     "to allow mutating tools",
            ),
        )
    # A write-capable tool called cross-origin from a browser page is the
    # CSRF-to-localhost vector (a text/plain POST needs no CORS preflight
    # to HAVE side effects) — reject before any handler runs. GET /tools
    # discovery is filtered to match (see list_tools).
    if _is_write_tool(tool) and not _origin_allowed(request):
        return JSONResponse(
            status_code=403,
            content=err_envelope(
                tool, "forbidden",
                "cross-origin write request rejected",
                hint="same-origin requests (or origins listed in "
                     "CIE_HTTP_ALLOWED_ORIGINS) are required for "
                     "mutating tools",
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
        payload = await run_in_threadpool(handler, body, project)
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
    except ModuleNotFoundError as exc:
        # Optional backend absent (e.g. `core.llm` from the protobox era):
        # a property of this installation, not a crash — 503, not 500.
        return JSONResponse(
            status_code=503,
            content=err_envelope(
                tool, "unavailable", f"{type(exc).__name__}: {exc}",
                hint="an optional backend module is not installed in this "
                     "standalone package; see the error message for which one",
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
def submit_tasks(batch: AtomicTaskBatch, project: str = Query(""),
                 _policy: None = Depends(_write_guard)) -> dict:
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
def set_task_status(name: str, update: StatusUpdateModel, project: str = Query(""),
                    _policy: None = Depends(_write_guard)) -> dict:
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
def add_artifact(name: str, artifact: Artifact, project: str = Query(""),
                 _policy: None = Depends(_write_guard)) -> dict:
    repo = factory.get_task_repo(_resolve_project(project))
    if not repo.add_artifact(name, artifact):
        raise HTTPException(status_code=404, detail=f"task '{name}' not found")
    return {"task": name, "path": artifact.path, "kind": artifact.kind.value}


@router.post("/tasks/{name}/events")
def add_event(name: str, event: RepairEvent, project: str = Query(""),
              _policy: None = Depends(_write_guard)) -> dict:
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
def code_reload(path: str = Query(...), project: str = Query(""),
                _policy: None = Depends(_write_guard)) -> dict:
    from cie.extract import extract_tree

    p = Path(path)
    if not p.exists():
        raise HTTPException(status_code=404, detail=f"path '{path}' not found")
    extraction = extract_tree(p)
    resolved_project = _resolve_project(project)
    engine = factory.get_engine(resolved_project)
    # 2026-08-14 fix (RF6): load_extraction's own docstring says empty
    # project replaces the WHOLE multi-project graph — this route resolved
    # `project` to pick the right engine above but never passed it through
    # to the write itself, so every reload wiped every OTHER project's data
    # too. Thread the same resolved project into the write.
    count = engine._repo.load_extraction(  # noqa: SLF001
        extraction.nodes, extraction.edges, project=resolved_project,
    )
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


def _synthesize_module_workflow_nodes(project_id: str, module_ids: list[str]) -> list[dict]:
    """Read-only, HierarchyNodeView-shaped pseudo-nodes for each Module's
    workflow narratives — the graph-view sibling of features/blueprint/
    graph_summary.py::_synthesize_workflow_entities (same underlying data,
    same reason it needs synthesizing: `workflow` is a bare `list[str]`
    field on :Module — core/graph/entities/module.py — never promoted to
    its own node type, so HierarchyNodeView's `_view_from_props` (which
    only reads id/name/description/metadata_json) silently drops it and
    it never reached this route at all before this).

    node_type is "module_workflow", deliberately NOT "workflow" — this
    module's own TYPE_TO_LABEL already maps "workflow" to a real :Workflow
    Neo4j label belonging to a distinct, unrelated "external planning
    system" schema (this file's module docstring). Nothing in this
    codebase writes that label today, but reusing its type key for an
    unrelated synthetic concept would be a real collision waiting to
    surface confusingly, not a hypothetical one, the moment it is.

    Queried directly rather than sourced from repo.get_project_tree()'s
    already-built HierarchyNodeView list, for the same reason: that list
    has already thrown the `workflow` property away by the time it exists."""
    if not module_ids:
        return []
    driver = factory.get_shared_driver()
    with driver.session() as session:
        rows = session.run(
            "MATCH (m:Module) WHERE m.id IN $ids "
            "RETURN m.id AS id, coalesce(m.module_name, m.name, m.id) AS module_name, "
            "coalesce(m.workflow, []) AS workflow",
            ids=module_ids,
        )
        module_rows = [dict(r) for r in rows]

    synthesized: list[dict] = []
    for row in module_rows:
        narratives = row["workflow"] or []
        module_id = row["id"]
        module_name = row["module_name"]
        for idx, narrative in enumerate(narratives):
            name = (
                f"{module_name} workflow" if len(narratives) == 1
                else f"{module_name} workflow {idx + 1}"
            )
            synthesized.append({
                "node_type": "module_workflow",
                "id": f"{module_id}::workflow::{idx}",
                "name": name,
                "description": narrative,
                "metadata": {},
                "depth": 0,
                "parent_id": module_id,
                "children_ids": [],
            })
    return synthesized


@router.get("/hierarchy/project/{project_id}")
def get_project_hierarchy(project_id: str) -> list[dict]:
    repo = factory.get_hierarchy_repo(project_id)
    nodes = [n.model_dump() for n in repo.get_project_tree(project_id)]
    module_ids = [n["id"] for n in nodes if n.get("node_type") == "module"]
    nodes.extend(_synthesize_module_workflow_nodes(project_id, module_ids))
    return nodes


@router.get("/api/projects/{project_id}/code-mapping")
def get_project_code_mapping(project_id: str) -> dict:
    """PRD->code mapping for the admin graph explorer: the same PRD tree
    get_project_hierarchy already returns (Module/Capability/UseCase/
    UserStory/Task — Task included, per that route's own coverage), plus
    every CodeFile node forge/generate_agent.py's write hook has written
    for this project, each tagged with the id of the Task that OUTPUTS it.
    Frontend stitches code_files onto their task_id — a project never
    rebuilt since this feature shipped will simply have an empty
    code_files list (fall back to each Task's own file_path there)."""
    repo = factory.get_hierarchy_repo(project_id)
    tree = [n.model_dump() for n in repo.get_project_tree(project_id)]

    driver = factory.get_shared_driver()
    with driver.session() as session:
        rows = session.run(
            "MATCH (c:CodeFile {project: $project_id}) "
            "OPTIONAL MATCH (t)-[:OUTPUTS]->(c) "
            "RETURN c.id AS id, c.file_path AS file_path, "
            "c.written_at AS written_at, c.status AS status, t.id AS task_id",
            project_id=project_id,
        )
        code_files = [dict(r) for r in rows]

    return {"tree": tree, "code_files": code_files}


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


# ---------------------------------------------------------------------------
# Blast radius (admin graph explorer, be-v2/docs/plans/admin-graph-
# explorer.md) — a plain REST alias for QueryEngine.affected_by, same
# direct-engine-call pattern as get_project_code_graph above (no
# ToolService fallback wrapper; this is a read-only admin view, not the
# repair agent's degraded-mode path). "incoming" (the default) answers
# "what breaks if I change this file"; "outgoing" answers "what does this
# file depend on".
# ---------------------------------------------------------------------------


@router.get("/api/projects/{project_id}/blast-radius")
def get_blast_radius(
    project_id: str,
    file_path: str = Query(...),
    direction: str = Query("incoming"),
    max_depth: int = Query(3, ge=1, le=6),
    max_results: int = Query(30, ge=1, le=50),
) -> dict:
    engine = factory.get_engine(project_id)
    hits = engine.affected_by(file_path, max_depth=max_depth, direction=direction, max_results=max_results)
    return {
        "file_path": file_path,
        "direction": direction,
        "hits": [
            {
                "id": h.node.id, "file": h.node.source_file, "symbol": h.node.label,
                "distance": h.distance, "confidence": h.confidence.value,
            }
            for h in hits
        ],
    }


# ---------------------------------------------------------------------------
# CIE Tool Console (admin graph explorer, be-v2/docs/plans/admin-graph-
# explorer.md) — a generic UI over CIE's ~90 read-only introspection/
# reporting tools (search, traceability, drift/clone reports, subsystem
# health, coverage, metrics, ...), reusing the existing POST /tools/{tool}
# dispatcher (`run_tool` above) for the actual call so there's exactly one
# envelope/error-handling implementation. This is an explicit ALLOWLIST,
# not a denylist of known-mutating tools: a tool this list doesn't
# recognize is excluded by default rather than accidentally exposed —
# deliberately excludes every tool that writes to the graph, filesystem,
# or test runner (write_file/edit_file/delete_file, reindex*, sync_*,
# every `*_run` analysis-trigger paired with a read-only report tool,
# record_*, push_hierarchy, install_git_hook, configure_layer_rules,
# start_watch/stop_watch, start_mock_server/stop_mock_server, run_tests,
# run, nook_and_corner_test (writes files by default), decompose_page,
# promote_hint_to_task, link_artifact, append_repair_events,
# inject_assertions/strip_assertions, export_rdf).
# ---------------------------------------------------------------------------

READ_ONLY_CIE_TOOLS: frozenset[str] = frozenset({
    "view_file", "get_meta", "get_function",
    "ls", "dir", "file_hierarchy", "file_names_like", "path_prefix",
    "search_symbol", "resolve_import", "semantic_search",
    "callers", "callees", "file_skeleton", "path_between", "failing_context",
    "affected_by", "class_hierarchy",
    "test_map", "actual_callers", "dead_code_confirm", "hybrid_search",
    "entity_context", "qa", "blame_history",
    "clone_clusters", "clone_find",
    "performance_profile", "antipattern_scan",
    "drift_report", "architecture_check",
    "metrics", "tech_debt_report", "metric_trend", "graph_diff",
    "community_search",
    "accuracy_check", "freshness_report", "comprehensiveness_report", "salience_report",
    "related_edges", "validate_property_constraints",
    "type_flow", "dependency_graph", "vulnerabilities", "doc_search", "contracts",
    "validate_types",
    "test_skeletons", "test_coverage",
    "state_machine", "fsm_validate",
    "traceability_coverage", "traceability_orphans", "traceability_chain",
    "prd_traceability_coverage", "prd_traceability_orphans", "prd_traceability_chain",
    "semantic_diff",
    "agent_verdicts", "confidence_report", "justification",
    "check_invariant", "invariant_violations",
    "telemetry_to_spec",
    "page_tree", "element_coverage", "implied_pages",
    "subsystem_health", "subsystem_gaps", "subsystem_dependency_graph",
    "population_path",
    "test_plan", "test_results", "coverage_gaps", "unified_coverage_report",
    "mock_registry", "mock_coverage", "mock_violations",
    "apm_metrics", "performance_baseline", "performance_regressions",
    "list_pending_tasks", "get_task", "task_dependency_closure",
    "get_coverage", "coverage_report", "coverage_trend",
    "get_children", "get_lineage", "get_layer_rules",
    "health", "schema_version",
    "validate_api_contracts", "validate_coverage", "validate_cycles",
})


@router.get("/api/cie-tools")
def list_cie_tools(project: str = Query("")) -> dict:
    """Manifest for the Tool Console dropdown: live-introspected {name,
    signature, doc} (same source as GET /tools) filtered down to
    READ_ONLY_CIE_TOOLS, so signatures/docs can't drift from what the
    tool actually accepts."""
    manifest = get_tool_service(project).describe()
    tools = [t for t in manifest.get("results", []) if t["name"] in READ_ONLY_CIE_TOOLS]
    return {"tools": tools}


@router.post("/api/cie-tools/{tool}")
async def run_cie_tool(tool: str, request: Request) -> JSONResponse:
    """Gated alias for POST /tools/{tool} — only forwards when `tool` is in
    the read-only allowlist above; everything else (including a tool this
    list has simply never heard of) gets a 403 rather than being forwarded
    to the full mutating-capable dispatcher."""
    if tool not in READ_ONLY_CIE_TOOLS:
        return JSONResponse(
            status_code=403,
            content=err_envelope(
                tool, "validation", f"'{tool}' is not exposed through the read-only Tool Console",
                hint="see READ_ONLY_CIE_TOOLS in cie/routes.py",
            ),
        )
    return await run_tool(tool, request)


# ---------------------------------------------------------------------------
# AI-01: GraphRAG question-answering (item 6 of the cie grounding slice —
# be-v2/docs/cie-grounding-slice-implementation.md). A plain REST alias for
# ToolService.qa alongside the generic POST /tools/qa dispatcher — trivial
# to add (Query params only, no new request/response Pydantic model) so it
# gets one, per that item's own scope note.
# ---------------------------------------------------------------------------


@router.post("/qa")
def qa_route(question: str = Query(...), project: str = Query("")) -> dict:
    return get_tool_service(project).qa(question)


# ---------------------------------------------------------------------------
# PS-14: multi-source event ingestion — one webhook receiver dispatching to
# the real sync primitives (cie.sync), per GraphSyncEvent's event_type.
# ---------------------------------------------------------------------------


class GraphSyncEventModel(BaseModel):
    event_type: str  # FILE_SAVE | COMMIT | CI_COMPLETE | REVERT
    commit_hash: str = ""
    file_paths: list[str] = []
    author: str = ""
    timestamp: str = ""


@router.post("/sync/event")
def sync_event(event: GraphSyncEventModel, project: str = Query(...),
               _policy: None = Depends(_write_guard)) -> dict:
    """PS-14 webhook receiver. Routes via `cie.sync.classify_event`:
    FILE_SAVE -> speculative reindex (no gate), COMMIT -> the full 4-stage
    gate per file, CI_COMPLETE -> promote speculative into canonical,
    REVERT -> soft-delete the commit's canonical nodes. `project` is
    required (unlike every other route here) — a sync event with no
    project would silently no-op against the empty-string default
    namespace, which is worse than a clear 422.
    """
    from cie import sync

    sync_event_obj = sync.GraphSyncEvent(
        event_type=event.event_type, commit_hash=event.commit_hash,
        file_paths=tuple(event.file_paths), author=event.author,
        timestamp=event.timestamp,
    )
    route = sync.classify_event(sync_event_obj)
    if route == "unknown":
        return {
            "ok": False, "tool": "sync_event",
            "error": {"kind": "validation", "message": f"unknown event_type {event.event_type!r}"},
            "hint": f"valid event types: {', '.join(sorted(sync._EVENT_ROUTES))}",  # noqa: SLF001
        }
    if route == "speculative":
        root = Path.cwd()
        spec_service = factory.build_tool_service(
            sync.speculative_project(project), root=root,
        )
        results = [spec_service.reindex_file(p) for p in event.file_paths]
        return {"ok": True, "tool": "sync_event", "route": route, "results": results}
    if route == "gate":
        service = get_tool_service(project)
        results = [service.sync_quality_gate(p) for p in event.file_paths]
        return {"ok": True, "tool": "sync_event", "route": route, "results": results}
    if route == "promote":
        result = get_tool_service(project).sync_promote(commit_hash=event.commit_hash)
        return {"ok": True, "tool": "sync_event", "route": route, "results": [result]}
    # route == "revert"
    result = get_tool_service(project).sync_revert(event.commit_hash)
    return {"ok": True, "tool": "sync_event", "route": route, "results": [result]}


# ---------------------------------------------------------------------------
# CI-15: Runtime Telemetry Ingestion (OTLP/HTTP JSON) — see cie/telemetry.py
# ---------------------------------------------------------------------------


@router.post("/telemetry/otlp")
async def ingest_telemetry(request: Request, project: str = Query(...),
                           _policy: None = Depends(_write_guard)) -> JSONResponse:
    """Real OTel span ingestion from a live deployment's own SDK — an
    external exporter's raw `ExportTraceServiceRequest` body (OTLP/HTTP
    JSON encoding), not cie's own `{project, ...kwargs}` tool envelope,
    so this is its own dedicated route rather than a `/tools/{tool}`
    entry. `project` is required as a query param (same reasoning as
    `/sync/event`): an exporter is typically configured with one fixed
    endpoint URL per deployment, and a telemetry batch silently landing
    in the empty-string default namespace would be much harder to
    notice than an immediate 422.

    See `cie/telemetry.py`'s module docstring for the wire-format and
    symbol-resolution scope (OTLP JSON only, `code.filepath`/
    `code.function` semantic-convention attributes only).
    """
    from cie import telemetry

    try:
        payload = await request.json()
    except Exception:  # noqa: BLE001 - malformed JSON body
        return JSONResponse(
            status_code=422,
            content=err_envelope(
                "ingest_telemetry", "validation", "request body must be a JSON object",
                hint="POST an OTLP/HTTP JSON ExportTraceServiceRequest body "
                     "(application/json, not protobuf)",
            ),
        )
    if not isinstance(payload, dict):
        return JSONResponse(
            status_code=422,
            content=err_envelope(
                "ingest_telemetry", "validation", "request body must be a JSON object",
            ),
        )

    resolved_project = _resolve_project(project)
    engine = factory.get_engine(resolved_project)
    spans = telemetry.parse_otlp_spans(payload)
    nodes, _edges = engine._repo.project_graph()  # noqa: SLF001
    index = telemetry.build_symbol_index(nodes)
    pairs, unresolved = telemetry.spans_to_actual_calls(spans, index)
    written = await run_in_threadpool(
        engine._repo.accumulate_actual_calls, pairs, resolved_project,  # noqa: SLF001
    )
    return JSONResponse(status_code=200, content=envelope(
        "ingest_telemetry",
        {
            "spans_received": len(spans),
            "edges_written": written,
            "unresolved_span_pairs": unresolved,
        },
        hint=(
            f"{unresolved} span pair(s) had no matching FUNC/METHOD node "
            "(missing/unmatched code.filepath+code.function attributes); "
            "actual_callers/dead_code_confirm results will be partial until "
            "your exporter's spans carry OTel's Semantic Conventions for "
            "Code attributes"
        ) if unresolved else None,
    ))
