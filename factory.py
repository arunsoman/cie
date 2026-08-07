"""One construction path for cie's engine/task-repo/tool-service.

Both the mounted API (cie/routes.py, project resolved per-request) and
in-process consumers (forge/tools.py's CieInProcessBackend) build their
cie objects here instead of each hand-rolling a QueryEngine +
Neo4jTaskRepository + ToolService, which is how the old standalone
cie/server.py and forge's HTTP client ended up drifting apart.

Engines are cached per project namespace (`project` is cie's
partitioning key inside the shared Neo4j database — see
cie/config.py's module docstring on why there's only one database).
A process that only ever touches one project (the cie CLI, a forge run
against one checkout) just gets one cache entry.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional

from cie.config import Neo4jConfig
from cie.query import QueryEngine
from cie.task_repository import Neo4jTaskRepository
from cie.tools import ToolService

_engines: dict[str, QueryEngine] = {}
_task_repos: dict[str, Neo4jTaskRepository] = {}
_shared_driver = None


def build_driver(cfg: Optional[Neo4jConfig] = None):
    """A fresh neo4j.Driver from config. Callers own its lifetime.

    Use get_shared_driver() instead unless a caller specifically needs its
    own unshared connection (e.g. a short-lived CLI invocation)."""
    from neo4j import GraphDatabase

    cfg = cfg or Neo4jConfig.from_env()
    return GraphDatabase.driver(cfg.uri, auth=(cfg.user, cfg.password), **cfg.driver_kwargs())


def get_shared_driver(cfg: Optional[Neo4jConfig] = None):
    """Process-wide Neo4j driver — the one connection cie's task/
    hierarchy repos AND the business graph (core.graph.client) write
    through. One shared driver, not a fresh TCP/bolt connection per write."""
    global _shared_driver
    if _shared_driver is None:
        _shared_driver = build_driver(cfg)
    return _shared_driver


def get_engine(project: str = "", cfg: Optional[Neo4jConfig] = None) -> QueryEngine:
    """Process-wide QueryEngine for one project namespace, cached.

    Built on `get_shared_driver()` rather than `Neo4jRepository.connect()`
    (which opens its own fresh `GraphDatabase.driver(...)`) — confirmed
    live 2026-08-07: those two ran as separate driver instances sharing
    nothing, so a server process ended up with two independent connection
    pools *and* two independently-refreshing routing tables against the
    same Aura instance. One shared driver means one pool, one routing
    table, refreshed once.
    """
    from cie.neo4j_repository import Neo4jRepository

    if project not in _engines:
        cfg = cfg or Neo4jConfig.from_env()
        _engines[project] = QueryEngine(
            Neo4jRepository(
                get_shared_driver(cfg), project=project,
                query_timeout_s=cfg.query_timeout_s,
                schema_timeout_s=cfg.schema_timeout_s,
                write_timeout_s=cfg.write_timeout_s,
            )
        )
    return _engines[project]


def get_task_repo(project: str = "", cfg: Optional[Neo4jConfig] = None) -> Neo4jTaskRepository:
    """Process-wide task repository for one project namespace, cached.

    One Neo4jTaskRepository instance per distinct project key, each bound
    to the same shared driver but constructed with its own project — so
    list_pending() (see that method) is scoped to this project, not the
    whole shared Neo4j.
    """
    if project not in _task_repos:
        _task_repos[project] = Neo4jTaskRepository.from_driver(get_shared_driver(cfg), project=project)
    return _task_repos[project]


def get_hierarchy_repo(project: str = "", cfg: Optional[Neo4jConfig] = None):
    from cie.hierarchy import Neo4jHierarchyRepository

    return Neo4jHierarchyRepository.from_driver(get_shared_driver(cfg), project=project)


def build_tool_service(
    project: str,
    root: Path,
    allowed_root: Optional[Path] = None,
    cfg: Optional[Neo4jConfig] = None,
) -> ToolService:
    """A ToolService scoped to one project namespace and one filesystem root."""
    return ToolService(
        get_engine(project, cfg),
        get_task_repo(project, cfg),
        root=Path(root),
        allowed_root=Path(allowed_root) if allowed_root is not None else None,
    )


def reset_caches() -> None:
    """Test/reload hook — drop cached engines, task repos, and the shared driver."""
    global _shared_driver
    _engines.clear()
    _task_repos.clear()
    _shared_driver = None
