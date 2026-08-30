"""Zero-config, embedded graph backend — no Neo4j required.

cie's Neo4j requirement is real setup
friction competitors in `docs/competitive-landscape.md` don't have
(CodeGraph: a single local SQLite file, nothing to configure).
`EmbeddedRepository` closes that gap for the "try it in one command"
path — it wraps `cie.in_memory_repository.InMemoryRepository`'s already-
verified query/traversal logic (the same class the Neo4j-backed test
suite has always checked its own semantics against) with durable
storage in one local SQLite file: two tables (`nodes`, `edges`), each row
holding that record's full dataclass serialized as JSON — inspectable
with a plain `sqlite3 graph.db "select * from nodes"`, not a hidden blob.

**Scope, stated plainly, not hidden:** this covers the full
`Repository` protocol (`InMemoryRepository` already implements all of
it) — same semantics as `Neo4jRepository` for the core graph-navigation
path (search, traversal, call graph, file skeleton) this backend exists
to make zero-config. It is not multi-process-safe (single SQLite
connection, no locking beyond SQLite's own) and every public call
re-persists the WHOLE graph rather than an incremental diff — a
deliberate simplicity-over-throughput tradeoff for a single-project,
local-first backend, not a production multi-tenant one. Reach for
`Neo4jRepository` (see `cie.factory.build_tool_service_from_config`) once
either of those stops being fine for your use case.
"""

from __future__ import annotations

import dataclasses
import json
import sqlite3
from pathlib import Path
from typing import Any

from cie.in_memory_repository import InMemoryRepository
from cie.models import Confidence, Edge, Node

_SCHEMA = """
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS edges (
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    relation TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
"""


def _node_to_row(node: Node) -> tuple[str, str]:
    return node.id, json.dumps(dataclasses.asdict(node))


def _row_to_node(data: str) -> Node:
    payload = json.loads(data)
    payload["embedding"] = tuple(payload.get("embedding") or ())
    return Node(**payload)


def _edge_to_row(edge: Edge) -> tuple[str, str, str, str]:
    payload = dataclasses.asdict(edge)
    payload["confidence"] = edge.confidence.value if isinstance(edge.confidence, Confidence) else edge.confidence
    return edge.source, edge.target, edge.relation, json.dumps(payload)


def _row_to_edge(data: str) -> Edge:
    payload = json.loads(data)
    payload["confidence"] = Confidence(payload.get("confidence", Confidence.EXTRACTED.value))
    return Edge(**payload)


def load_graph(db_path: Path) -> tuple[list[Node], list[Edge]]:
    """Read every node/edge out of `db_path`, or return ``([], [])`` for a
    fresh/nonexistent file — the "first run, empty project" case, not an
    error."""
    if not db_path.exists():
        return [], []
    conn = sqlite3.connect(str(db_path))
    try:
        conn.executescript(_SCHEMA)
        nodes = [_row_to_node(row[0]) for row in conn.execute("SELECT data FROM nodes")]
        edges = [_row_to_edge(row[0]) for row in conn.execute("SELECT data FROM edges")]
        return nodes, edges
    finally:
        conn.close()


def save_graph(db_path: Path, nodes: list[Node], edges: list[Edge]) -> None:
    """Overwrite `db_path`'s nodes/edges tables with the given full graph
    state — the whole graph, not a diff (see this module's docstring)."""
    db_path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(db_path))
    try:
        conn.executescript(_SCHEMA)
        conn.execute("DELETE FROM nodes")
        conn.execute("DELETE FROM edges")
        conn.executemany(
            "INSERT INTO nodes (id, data) VALUES (?, ?)",
            (_node_to_row(n) for n in nodes),
        )
        conn.executemany(
            "INSERT INTO edges (source, target, relation, data) VALUES (?, ?, ?, ?)",
            (_edge_to_row(e) for e in edges),
        )
        conn.commit()
    finally:
        conn.close()


class _TaskTrackingUnavailable(RuntimeError):
    def __init__(self, method: str):
        super().__init__(
            f"cie.tools.ToolService.{method}: task tracking was disabled "
            "for this embedded ToolService (build_tool_service_embedded"
            "(..., task_tracking=False), or --no-task-tracking on cie-mcp) "
            "— drop that flag to get cie.embedded_task_repository."
            "EmbeddedTaskRepository instead, or configure Neo4j "
            "(cie.factory.build_tool_service_from_config with a "
            "CieConfig.neo4j) for the full multi-project store."
        )


class NullTaskRepository:
    """A `TaskRepository` (`cie.task_repository.TaskRepository`) that fails
    fast and clearly on every method — the explicit opt-out for a caller
    that passes `task_tracking=False` to `build_tool_service_embedded`
    (or `--no-task-tracking` to `cie-mcp --embedded`) and wants the
    smallest possible footprint with no `tasks.db` file at all. The
    embedded backend's *default* is
    `cie.embedded_task_repository.EmbeddedTaskRepository`, a real
    SQLite-backed implementation — this class is no longer what most
    callers get. Still fails fast and clearly on every method rather than
    silently returning empty results a caller could mistake for "no tasks
    exist yet", per this codebase's own fail-fast convention (see e.g.
    `cie.decompose`'s registration seam)."""

    def __getattr__(self, name: str):
        if name.startswith("_"):
            raise AttributeError(name)

        def _raise(*args: Any, **kwargs: Any) -> Any:
            raise _TaskTrackingUnavailable(name)

        return _raise


class EmbeddedRepository(InMemoryRepository):
    """`InMemoryRepository`, durable across process restarts via one local
    SQLite file.

    Every public method call re-persists the current graph state
    afterward — deliberately unconditional (not "only the methods we
    remembered are writes") after this session found a write-
    classification list (`cie.tool_policy.WRITE_TOOLS`) had silently
    missed a real mutating method once already; a fixed, small, hand-
    maintained list is exactly the bug class this avoids by construction,
    at the cost of a redundant flush on pure-read calls. Fine for a
    single local project's graph; revisit if that cost ever matters for a
    real use case.
    """

    def __init__(self, db_path: Path | str, project: str = ""):
        self._db_path = Path(db_path)
        nodes, edges = load_graph(self._db_path)
        super().__init__(nodes, edges)
        self._project = project
        self._flush()  # ensure the file (and its schema) exists after first construction

    def _flush(self) -> None:
        save_graph(self._db_path, list(self._nodes.values()), list(self._edges))

    def load_extraction(self, nodes, edges, project: str = "") -> int:
        """Embedded edition of `Neo4jRepository._maybe_compute_embeddings`:
        when an embeddings implementation is available (host `core.llm`,
        a `register_embed_functions` override, or the env-gated
        OpenAI-compatible fallback — see `cie.embed.supports_embeddings`),
        enrich the incoming extraction node dicts with real vectors
        BEFORE they become persistent `Node`s; the post-call flush then
        persists them like any other write. When nothing is configured,
        rows skip embedding computation entirely (no network, no error)
        — same guard, embedded backend."""
        from cie.embed import compute_embeddings, supports_embeddings

        if supports_embeddings() and nodes:
            try:
                compute_embeddings(list(nodes))
            except Exception:  # noqa: BLE001 - de-embed, never fail the load
                logging.getLogger("cie.embedded_repository").warning(
                    "embedding enrichment failed for %d node(s); "
                    "indexing continues without vectors", len(nodes),
                    exc_info=True,
                )
        return super().load_extraction(nodes, edges, project)

    def __getattribute__(self, name: str) -> Any:
        attr = object.__getattribute__(self, name)
        if name.startswith("_") or not callable(attr):
            return attr

        def _flush_after(*args: Any, **kwargs: Any) -> Any:
            result = attr(*args, **kwargs)
            object.__getattribute__(self, "_flush")()
            return result

        return _flush_after
