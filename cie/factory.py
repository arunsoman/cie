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
    from cie.graph_cache import get_query_cache
    from cie.neo4j_repository import Neo4jRepository

    if project not in _engines:
        cfg = cfg or Neo4jConfig.from_env()
        _engines[project] = QueryEngine(
            Neo4jRepository(
                get_shared_driver(cfg), project=project,
                query_timeout_s=cfg.query_timeout_s,
                schema_timeout_s=cfg.schema_timeout_s,
                write_timeout_s=cfg.write_timeout_s,
                query_cache=get_query_cache(project),
            )
        )
    return _engines[project]


def get_task_repo(project: str = "", cfg: Optional[Neo4jConfig] = None) -> Neo4jTaskRepository:
    """Process-wide task repository for one project namespace, cached.

    One Neo4jTaskRepository instance per distinct project key, each bound
    to the same shared driver but constructed with its own project — so
    list_pending() (see that method) is scoped to this project, not the
    whole shared Neo4j.

    Wired with a real write-behind entity cache (docs/plans/graph-write-
    behind-cache-plan.md) — this is the one production construction path
    that gets one; direct `Neo4jTaskRepository(driver, project=...)`
    construction (every existing test in this suite, and any future
    caller that wants the old uncached behavior) does not.
    """
    if project not in _task_repos:
        from cie.graph_cache import get_entity_cache
        from cie.task_repository import flush_atomic_task_batch

        driver = get_shared_driver(cfg)
        entity_cache = get_entity_cache(
            project,
            flush_fns={"AtomicTask": lambda rows, _d=driver: flush_atomic_task_batch(_d, rows)},
        )
        _task_repos[project] = Neo4jTaskRepository.from_driver(
            driver, project=project, entity_cache=entity_cache,
        )
    return _task_repos[project]


def get_hierarchy_repo(project: str = "", cfg: Optional[Neo4jConfig] = None):
    from cie.hierarchy import Neo4jHierarchyRepository

    return Neo4jHierarchyRepository.from_driver(get_shared_driver(cfg), project=project)


def build_tool_service(
    project: str,
    root: Path,
    allowed_root: Optional[Path] = None,
    cfg: Optional[Neo4jConfig] = None,
    max_file_size_bytes: Optional[int] = None,
) -> ToolService:
    """A ToolService scoped to one project namespace and one filesystem root.

    `max_file_size_bytes` (docs/plans/cie-standalone-any-project-plan.md
    Phase 4) is optional here — `None` lets `ToolService.__init__` use its
    own default rather than this function needing to know that default.
    """
    kwargs: dict = {}
    if max_file_size_bytes is not None:
        kwargs["max_file_size_bytes"] = max_file_size_bytes
    return ToolService(
        get_engine(project, cfg),
        get_task_repo(project, cfg),
        root=Path(root),
        allowed_root=Path(allowed_root) if allowed_root is not None else None,
        project=project,
        **kwargs,
    )


def build_tool_service_from_config(config: "CieConfig") -> ToolService:
    """One-call, no-env-vars-required bootstrap for an external ("any
    project") caller — see `cie.config.CieConfig`.

    Bundles what `build_tool_service` above already exposed as separate
    args, plus registering `config.language_adapters` (Phase 1) into the
    process-wide registry before building the engine, so a host project's
    own adapter is live for this call and every later one in this
    process — `cie.lang_adapter.register_adapter` is itself already a
    process-wide registry, not per-ToolService state, so this is the
    correct place to apply it once, not something `ToolService.__init__`
    itself should do.
    """
    from cie import lang_adapter

    for adapter in config.language_adapters:
        lang_adapter.register_adapter(adapter)
    return build_tool_service(
        config.project,
        config.project_root,
        allowed_root=config.allowed_root,
        cfg=config.neo4j,
        max_file_size_bytes=config.max_file_size_bytes,
    )


def build_tool_service_embedded(
    root: Path,
    db_path: Optional[Path] = None,
    project: str = "",
    max_file_size_bytes: Optional[int] = None,
    task_db_path: Optional[Path] = None,
    task_tracking: bool = True,
) -> ToolService:
    """Zero-config `ToolService`, no Neo4j required — the other half of
    `build_tool_service`/`build_tool_service_from_config` above, backed by
    `cie.embedded_repository.EmbeddedRepository` (a local SQLite file,
    default `<root>/.cie/graph.db`) instead of `Neo4jRepository`.

    Task/QA tracking (`docs/growth-plan.md` Phase 0.5, workstream B) is
    now available here too: `cie.embedded_task_repository.
    EmbeddedTaskRepository`, a second local SQLite file (default
    `<root>/.cie/tasks.db`), the same `TaskRepository` protocol
    `Neo4jTaskRepository` implements. Pass `task_tracking=False` to fall
    back to `cie.embedded_repository.NullTaskRepository` (fail-fast on
    every task call) instead — e.g. for a caller that wants the smallest
    possible footprint and never touches task tools.
    """
    from cie.embedded_repository import EmbeddedRepository, NullTaskRepository
    from cie.embedded_task_repository import EmbeddedTaskRepository
    from cie.query import QueryEngine

    resolved_root = Path(root)
    resolved_db = Path(db_path) if db_path is not None else resolved_root / ".cie" / "graph.db"
    engine = QueryEngine(EmbeddedRepository(resolved_db, project=project))
    kwargs: dict = {}
    if max_file_size_bytes is not None:
        kwargs["max_file_size_bytes"] = max_file_size_bytes
    if task_tracking:
        resolved_task_db = (
            Path(task_db_path) if task_db_path is not None
            else resolved_root / ".cie" / "tasks.db"
        )
        task_repo = EmbeddedTaskRepository(resolved_task_db, project=project)
    else:
        task_repo = NullTaskRepository()
    return ToolService(
        engine, task_repo, root=resolved_root, project=project, **kwargs,
    )


def reset_caches() -> None:
    """Test/reload hook — drop cached engines, task repos, the shared
    driver, and the graph_cache entity/query cache singletons (stops
    every entity cache's flusher thread first)."""
    global _shared_driver
    from cie.graph_cache import reset_caches as _reset_graph_caches

    _engines.clear()
    _task_repos.clear()
    _shared_driver = None
    _reset_graph_caches()
