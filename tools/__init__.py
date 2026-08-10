"""Tool layer facade: one consolidated action per intent.

:class:`ToolService` composes the pure filesystem/subprocess/git helpers in
this package with the query engine (SPEC §3, Wave B1) and the task
repository (SPEC §5.2, Wave B2). Every method returns an envelope-ready
plain dict matching the SPEC §0 standard envelope::

    {"ok": true, "tool": ..., "results": [...], "truncated": false,
     "total": N, "hint": ..., "elapsed_ms": N}

Errors use ``{"ok": false, "tool": ..., "error": {"kind", "message"},
"hint": ..., "elapsed_ms": N}``. Hints are MANDATORY on empty results and
errors (integration spec §0.3). The surface layer (CLI/HTTP, Wave C) serializes
these dicts verbatim.

Interface assumptions (to verify at integration time):

* ``engine``: SPEC §3 QueryEngine — ``search_symbols(name, kind, file_glob,
  limit) -> list[SymbolMatch]``, ``get_callers(symbol, limit) ->
  list[EdgeRecord]``, ``get_callees(symbol, limit) -> list[EdgeRecord]``,
  ``get_file_skeleton(path) -> list[Node]``, ``failing_context(test, depth,
  limit) -> list[ContextHit]``, plus HEAD methods ``shortest_path(source,
  target, max_hops) -> Optional[PathResult]`` and ``get_node(label_or_id) ->
  Optional[NodeRecord]``.
* ``engine._repo.reindex_file(path, extraction, project="") -> int`` —
  the repository performs its own pass-2 call resolution internally (it
  calls ``callgraph.resolve_call_edges``; both exist at HEAD), so the
  service only supplies the fresh ``extract.extract_file`` Extraction.
* ``task_repo``: SPEC §5.2 — ``list_pending() -> list[AtomicTask]``,
  ``get_task(name) -> Optional[AtomicTask]``, ``get_dependent_tasks(name) ->
  list[AtomicTask]``, ``artifacts_for_path(path) -> list[tuple[str,
  Artifact]]``.
"""

from __future__ import annotations

import asyncio
import dataclasses
import hashlib
import inspect
import logging
import time
from pathlib import Path
from typing import Any, Optional

from cie import extract
from cie import serialize as _serialize
from cie.models import Confidence, Node, SymbolMatch

from cie.tools import blame as _blame
from cie.tools import edit as _edit
from cie.tools import runner as _runner
from cie.tools import view as _view
from cie.tools.heuristic import HeuristicToolSet
from cie.tools.index import Symbol, SymbolIndex

logger = logging.getLogger("cie.tools")

__all__ = [
    "ToolService",
    "RunResult",
    "run_command",
    "view_file",
    "blame_history",
    "write_file",
    "edit_file",
    "delete_file",
]

RunResult = _runner.RunResult
run_command = _runner.run_command
view_file = _view.view_file
blame_history = _blame.blame_history
write_file = _edit.write_file
edit_file = _edit.edit_file
delete_file = _edit.delete_file

# Canonical hint strings (SPEC §3 / integration spec §0.3).
HINT_EMPTY_CALLERS = (
    "call-graph coverage is partial until pass-2 inference runs; empty != unused"
)
HINT_EMPTY_FAILING_CONTEXT = "test node not indexed; load/reindex the test file first"


def _elapsed_ms(started: float) -> int:
    """Whole milliseconds since ``started`` (a time.monotonic() reading)."""
    return int(round((time.monotonic() - started) * 1000))


def _confidence_value(confidence: Any) -> str:
    """String value of a Confidence enum member (or plain string)."""
    return str(getattr(confidence, "value", confidence))


def _symbol_to_node(s: Symbol) -> Node:
    """Adapt a `cie.tools.index.Symbol` (the in-memory heuristic index's own
    shape) to a `cie.models.Node`, so the heuristic fallback path can feed
    the same graph-shaped consumers (`_view.view_file`'s skeleton param,
    `search_symbol`/`file_skeleton`'s result-shaping) the real graph path
    already uses — one result-shaping code path, not two."""
    return Node(
        id=f"{s.file}::{s.name}",
        label=s.name,
        source_file=s.file,
        kind=s.kind,
        signature=s.signature,
        line_start=s.start,
        line_end=s.end,
        docstring=s.doc,
    )


def _task_to_dict(task: Any) -> dict:
    """JSON-ready dict for an AtomicTask (pydantic) or a simple stub object."""
    model_dump = getattr(task, "model_dump", None)
    if callable(model_dump):
        return model_dump(mode="json")
    if dataclasses.is_dataclass(task) and not isinstance(task, type):
        return dataclasses.asdict(task)
    return dict(vars(task))


class ToolService:
    """Facade over the query engine, task repo, and fs/subprocess/git tools.

    Args:
        engine: QueryEngine per SPEC §3 (see module docstring).
        task_repo: TaskRepository per SPEC §5.2 (see module docstring).
        root: Project root; file views, runs, and git history are jailed
            under it.
        allowed_root: Jail root for the ``run`` tool's cwd. Defaults to
            ``root``. Pass a parent directory to allow running in sibling
            directories of ``root``.
    """

    def __init__(
        self,
        engine: Any,
        task_repo: Any,
        root: Path,
        allowed_root: Optional[Path] = None,
    ) -> None:
        self._engine = engine
        self._task_repo = task_repo
        self._root = Path(root)
        self._allowed_root = (
            Path(allowed_root) if allowed_root is not None else self._root
        )
        # Populated by start_watch()/stopped by stop_watch(); an in-process
        # watchdog.observers.Observer, not the SEPARATE `cie watch`
        # subprocess forge's CieBackend spawns for its own in-process use
        # (forge/tools.py) — see cie.tools.watch's module docstring for why
        # both exist and share one handler implementation.
        self._watch_observer: Any = None
        # Lazily built by _heuristic_fallback() below — NOT built here in
        # __init__, since most ToolService instances never need it (Neo4j
        # serves nearly every real call); a full `SymbolIndex` project
        # walk+parse on every construction (every project's first-touch
        # engine build, per cie.factory's caching) would be pure waste for
        # the common case. Built on first actual need — a graph call that
        # fails or comes back empty — and kept alive afterward so
        # write_file/edit_file/delete_file/reindex_file can keep it
        # incrementally fresh (see those methods) instead of it silently
        # going stale for the rest of this ToolService's life.
        self._symbol_index: Optional[SymbolIndex] = None
        self._heuristic: Optional[HeuristicToolSet] = None
        # path -> sha256 of the content this file was last indexed with.
        # Populated by reindex_file() (so write_file/edit_file/delete_file's
        # _sync_graph_after_write keeps it warm for free) and consulted by
        # reindex() to skip files whose content hasn't changed since their
        # last index instead of re-parsing/re-embedding every file under
        # project_dir on every call — see reindex()'s docstring.
        self._indexed_hashes: dict[str, str] = {}

    # -- heuristic fallback ---------------------------------------------------

    def _heuristic_fallback(self) -> HeuristicToolSet:
        if self._heuristic is None:
            self._symbol_index = SymbolIndex(self._root)
            self._heuristic = HeuristicToolSet(self._root, self._symbol_index)
        return self._heuristic

    def _try_graph(self, tool: str, fn, *args, **kwargs) -> tuple[Any, Optional[Exception]]:
        """Call a graph-backed retrieval, returning ``(result, exc)`` —
        ``exc`` is ``None`` on success. Never raises itself: callers decide
        whether/how to fall back to the heuristic index. Always logs a
        genuine failure at WARNING — falling back silently would mean a
        real Neo4j outage or a real bug in a query never shows up anywhere,
        indistinguishable from "this project just has no data for this
        query yet." A caller degrading gracefully doesn't mean the failure
        should be invisible.
        """
        try:
            return fn(*args, **kwargs), None
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                "%s: graph query failed (%s: %s); falling back to heuristic index",
                tool, type(exc).__name__, exc,
            )
            return None, exc

    @staticmethod
    def _degraded_hint(exc: Exception, hint: Optional[str]) -> str:
        base = f"graph unreachable ({type(exc).__name__}: {exc}); heuristic fallback used"
        return f"{base} — {hint}" if hint else base

    # -- envelope helpers ---------------------------------------------------

    @staticmethod
    def _ok(
        tool: str,
        results: Any,
        started: float,
        *,
        truncated: bool = False,
        total: Optional[int] = None,
        hint: Optional[str] = None,
    ) -> dict:
        """Build the SPEC §0 success envelope."""
        if total is None:
            total = len(results) if isinstance(results, list) else 1
        return {
            "ok": True,
            "tool": tool,
            "results": results,
            "truncated": truncated,
            "total": total,
            "hint": hint,
            "elapsed_ms": _elapsed_ms(started),
        }

    @staticmethod
    def _err(
        tool: str,
        kind: str,
        message: str,
        started: float,
        *,
        hint: Optional[str] = None,
    ) -> dict:
        """Build the SPEC §0 error envelope."""
        return {
            "ok": False,
            "tool": tool,
            "error": {"kind": kind, "message": message},
            "hint": hint,
            "elapsed_ms": _elapsed_ms(started),
        }

    def _guard(self, tool: str, started: float, exc: Exception) -> dict:
        """Convert tool exceptions into error envelopes."""
        if isinstance(exc, FileNotFoundError):
            return self._err(tool, "not_found", str(exc), started,
                             hint="check the path with search_symbol or file_skeleton")
        if isinstance(exc, ValueError):
            return self._err(tool, "validation", str(exc), started,
                             hint="paths and working directories must stay "
                                  "inside the project root")
        return self._err(tool, "internal", f"{type(exc).__name__}: {exc}",
                         started, hint="unexpected tool failure; report this")

    # -- discovery ------------------------------------------------------------

    def describe(self) -> dict:
        """Introspects every public tool method on this class via
        `inspect.signature` + its docstring's first line (docs/forge-
        rebuild-plan.md's Phase 2 / WS3, closes review finding P1.6).

        This is deliberately NOT a hand-maintained list — the exact bug
        class this whole rebuild exists to fix for the SEARCH/REPLACE
        parser (three independent hand-rolled copies that drifted). Adding
        a new public method to this class makes it show up here with zero
        extra maintenance; `tests/` enforces zero drift against
        `cie.routes.TOOLS`'s `_service_tool`-backed keys (see that
        test's docstring for why the comparison lives in a test, not
        runtime code — importing `cie.routes` from here would invert
        this package's layering).

        Called once, at run initialization (forge's scaffold phase), not
        during the repair loop itself — see `forge/tools.py::ToolBackend.
        describe()` and `forge/agent.py`'s module docstring for how the
        cached manifest reaches the agent as read-only context instead of
        a live call the agent could make mid-loop.
        """
        tool = "describe"
        started = time.monotonic()
        manifest = []
        for name in sorted(vars(type(self))):
            if name.startswith("_") or name == "describe":
                continue
            attr = vars(type(self))[name]
            if not callable(attr):
                continue
            method = getattr(self, name)
            doc = ""
            if method.__doc__:
                doc = method.__doc__.strip().splitlines()[0]
            try:
                signature = str(inspect.signature(method))
            except (TypeError, ValueError):
                signature = "(...)"
            manifest.append({"name": name, "signature": signature, "doc": doc})
        return self._ok(tool, manifest, started,
                        hint="call any named tool directly as a ToolService method, "
                             "or POST /tools/{name} with these kwargs over HTTP")

    # -- graph-backed tools -------------------------------------------------

    def view_file(self, path: str, start: int = 1, end: int = 100) -> dict:
        """Windowed file view joined with the graph skeleton (T1.1).

        The window's CONTENT always comes straight off disk (`_view.
        view_file` reads the real file), regardless of graph state — only
        the `symbol_index` annotation is graph-backed, so a Neo4j failure
        here degrades to the in-memory heuristic index for that annotation
        rather than failing the whole view.
        """
        tool = "view_file"
        started = time.monotonic()
        skeleton, exc = self._try_graph(tool, self._engine.get_file_skeleton, path)
        if exc is not None or not skeleton:
            skeleton = [
                _symbol_to_node(s) for s in self._heuristic_fallback().index.in_file(path)
            ]
        try:
            result = _view.view_file(self._root, path, start, end, skeleton)
        except Exception as exc2:  # noqa: BLE001 - converted to envelope
            return self._guard(tool, started, exc2)
        window = result["window"]
        truncated = window["end"] < result["total_lines"]
        hint = result["hint"]
        if exc is not None:
            hint = self._degraded_hint(exc, hint)
        return self._ok(tool, [result], started, truncated=truncated, hint=hint)

    def search_symbol(
        self,
        name: str,
        kind: str = "",
        file_glob: str = "",
        limit: int = 20,
    ) -> dict:
        """Locate symbol definitions by name (T1.2)."""
        tool = "search_symbol"
        started = time.monotonic()
        matches, exc = self._try_graph(
            tool, self._engine.search_symbols, name, kind, file_glob, limit,
        )
        if exc is not None or not matches:
            heuristic_hits = self._heuristic_fallback().index.find(name, kind)[:limit]
            if heuristic_hits:
                matches = [
                    SymbolMatch(
                        node=_symbol_to_node(s),
                        score=2.0 if s.name == name else 1.0,
                        confidence=Confidence.EXTRACTED if s.file.endswith(".py") else Confidence.INFERRED,
                    )
                    for s in heuristic_hits
                ]
        results = [
            {
                "name": m.node.label,
                "kind": m.node.kind,
                "signature": m.node.signature,
                "source_file": m.node.source_file,
                "line_range": [m.node.line_start, m.node.line_end],
                "confidence": _confidence_value(m.confidence),
                "score": m.score,
            }
            for m in (matches or [])
        ]
        if not results:
            hint = f"no symbol named '{name}'; try a substring or drop the kind filter"
            if exc is not None:
                hint = self._degraded_hint(exc, hint)
            return self._ok(tool, results, started, hint=hint)
        truncated = len(results) >= limit
        hint = (
            f"results capped at {limit}; refine with kind or file_glob"
            if truncated
            else None
        )
        if exc is not None:
            hint = self._degraded_hint(exc, hint)
        return self._ok(tool, results, started, truncated=truncated, hint=hint)

    def semantic_search(self, query: str, top_k: int = 10) -> dict:
        """Rank nodes by embedding similarity to a natural-language query."""
        tool = "semantic_search"
        started = time.monotonic()
        try:
            matches = self._engine.semantic_search(query, top_k=top_k)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "name": m.node.label,
                "kind": m.node.kind,
                "signature": m.node.signature,
                "source_file": m.node.source_file,
                "line_range": [m.node.line_start, m.node.line_end],
                "score": m.score,
            }
            for m in matches
        ]
        if not results:
            return self._ok(
                tool, results, started,
                hint="no embeddings matched; nodes must be loaded/reindexed "
                     "with NVIDIA_API_KEY set before semantic_search has "
                     "anything to rank - try search_symbol for lexical "
                     "matching instead",
            )
        truncated = len(results) >= top_k
        hint = (
            f"results capped at {top_k}; raise top_k for more"
            if truncated
            else None
        )
        return self._ok(tool, results, started, truncated=truncated, hint=hint)

    def _edge_results(self, records: Any, direction: str) -> list[dict]:
        """Shape EdgeRecords into caller/callee result dicts.

        Node file/signature are enriched best-effort via ``engine.get_node``
        (HEAD QueryEngine method) so results match the T1.3 shape.
        """
        results: list[dict] = []
        for record in records:
            peer_label = (
                record.source_label if direction == "caller" else record.target_label
            )
            peer = None
            get_node = getattr(self._engine, "get_node", None)
            if callable(get_node):
                node_record = get_node(peer_label)
                if node_record is not None:
                    peer = node_record.node
            entry = {
                direction: peer_label,
                "relation": record.edge.relation,
                "confidence": _confidence_value(record.edge.confidence),
            }
            if peer is not None:
                entry[f"{direction}_file"] = peer.source_file
                entry[f"{direction}_signature"] = peer.signature
            results.append(entry)
        return results

    def _graph_result_or_heuristic(
        self, tool: str, started: float, records: Any, exc: Optional[Exception],
        heuristic_call: Any, empty_hint: str,
    ) -> Optional[dict]:
        """Shared tail for callers/callees/failing_context/affected_by:
        given the outcome of an already-attempted `_try_graph` call, either
        shape the real graph records into a success envelope, or fall back
        to the heuristic index (on a genuine failure, or on a legitimately
        empty graph result if the heuristic index has something to offer
        instead). Returns ``None`` when the graph result should be used —
        the caller shapes and returns its own envelope in that case, since
        each tool's result shape differs enough that generalizing it too
        wouldn't be worth the indirection.
        """
        if exc is None and records:
            return None
        fallback = heuristic_call()
        if fallback["results"]:
            if exc is not None:
                fallback["hint"] = self._degraded_hint(exc, fallback.get("hint"))
            return fallback
        hint = empty_hint if exc is None else self._degraded_hint(exc, empty_hint)
        return self._ok(tool, [], started, hint=hint)

    def callers(self, symbol: str, limit: int = 30) -> dict:
        """Blast radius: who calls ``symbol`` (T1.3)."""
        tool = "callers"
        started = time.monotonic()
        records, exc = self._try_graph(tool, self._engine.get_callers, symbol, limit)
        fallback = self._graph_result_or_heuristic(
            tool, started, records, exc,
            heuristic_call=lambda: self._heuristic_fallback().callers(symbol),
            empty_hint=HINT_EMPTY_CALLERS,
        )
        if fallback is not None:
            return fallback
        results = self._edge_results(records, "caller")
        truncated = len(results) >= limit
        return self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {limit}" if truncated else None,
        )

    def callees(self, symbol: str, limit: int = 30) -> dict:
        """Reverse localization: what ``symbol`` calls (T1.3)."""
        tool = "callees"
        started = time.monotonic()
        records, exc = self._try_graph(tool, self._engine.get_callees, symbol, limit)
        fallback = self._graph_result_or_heuristic(
            tool, started, records, exc,
            heuristic_call=lambda: self._heuristic_fallback().callees(symbol),
            empty_hint=HINT_EMPTY_CALLERS,
        )
        if fallback is not None:
            return fallback
        results = self._edge_results(records, "callee")
        truncated = len(results) >= limit
        return self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {limit}" if truncated else None,
        )

    def file_skeleton(self, path: str) -> dict:
        """Signatures + line ranges for every symbol in a file, no bodies (T2.1)."""
        tool = "file_skeleton"
        started = time.monotonic()
        nodes, exc = self._try_graph(tool, self._engine.get_file_skeleton, path)
        if exc is not None or not nodes:
            heuristic_syms = self._heuristic_fallback().index.in_file(path)
            if heuristic_syms:
                nodes = [_symbol_to_node(s) for s in heuristic_syms]
        symbols = [
            {
                "name": n.label,
                "kind": n.kind,
                "signature": n.signature,
                "lines": [n.line_start, n.line_end],
                "docstring_first_line": n.docstring,
            }
            for n in (nodes or [])
            if n.kind != "file"  # file hub is not a symbol
        ]
        result = {"path": path, "symbols": symbols}
        hint = None if symbols else f"no symbols indexed for '{path}'; load/reindex the file first"
        if exc is not None:
            hint = self._degraded_hint(exc, hint)
        return self._ok(tool, [result], started, hint=hint)

    def failing_context(
        self, test_identifier: str, depth: int = 3, limit: int = 30
    ) -> dict:
        """Symbols reachable from a failing test, ranked by distance (T2.3)."""
        tool = "failing_context"
        started = time.monotonic()
        hits, exc = self._try_graph(
            tool, self._engine.failing_context, test_identifier, depth, limit,
        )
        fallback = self._graph_result_or_heuristic(
            tool, started, hits, exc,
            heuristic_call=lambda: self._heuristic_fallback().failing_context(test_identifier),
            empty_hint=HINT_EMPTY_FAILING_CONTEXT,
        )
        if fallback is not None:
            return fallback
        results = [
            {
                "distance": h.distance,
                "file": h.node.source_file,
                "symbol": h.node.label,
                "confidence": _confidence_value(h.confidence),
            }
            for h in hits
        ]
        return self._ok(tool, results, started,
                        hint="distance-1 symbols are the prime suspects")

    def path_between(self, source: str, target: str, max_hops: int = 8) -> dict:
        """Shortest call/contains path as ordered per-hop chain (T2.2)."""
        tool = "path_between"
        started = time.monotonic()
        path, exc = self._try_graph(tool, self._engine.shortest_path, source, target, max_hops)
        empty_hint = (
            f"no path between '{source}' and '{target}' within {max_hops} hops; "
            "check the symbol names or raise max_hops"
        )
        if exc is not None or path is None or not path.nodes:
            fallback = self._heuristic_fallback().path_between(source, target)
            if fallback["results"]:
                if exc is not None:
                    fallback["hint"] = self._degraded_hint(exc, fallback.get("hint"))
                return fallback
            hint = empty_hint if exc is None else self._degraded_hint(exc, empty_hint)
            return self._ok(tool, [], started, hint=hint)
        chain: list[dict] = []
        for index, node in enumerate(path.nodes):
            hop = {"file": node.source_file, "symbol": node.label}
            if index < len(path.edges):
                edge = path.edges[index].edge
                hop["relation"] = edge.relation
                hop["confidence"] = _confidence_value(edge.confidence)
            else:
                hop["relation"] = None
                hop["confidence"] = None
            chain.append(hop)
        return self._ok(tool, [{"hops": path.hops, "chain": chain}], started)

    def affected_by(self, file_path: str, max_depth: int = 3,
                    direction: str = "incoming", max_results: int = 30) -> dict:
        """Blast radius (docs/forge-rebuild-plan.md's Phase 2 / WS4): what
        depends on `file_path` (direction="incoming", the default) or what
        it depends on (direction="outgoing")."""
        tool = "affected_by"
        started = time.monotonic()
        hits, exc = self._try_graph(
            tool, self._engine.affected_by, file_path, max_depth, direction, max_results,
        )
        empty_hint = (
            f"no nodes found for '{file_path}' within {max_depth} hops "
            f"({direction}); check the path or raise max_depth"
        )
        fallback = self._graph_result_or_heuristic(
            tool, started, hits, exc,
            heuristic_call=lambda: self._heuristic_fallback().affected_by(
                file_path, max_depth, direction,
            ),
            empty_hint=empty_hint,
        )
        if fallback is not None:
            return fallback
        results = [
            {
                "distance": h.distance,
                "file": h.node.source_file,
                "symbol": h.node.label,
                "confidence": _confidence_value(h.confidence),
            }
            for h in hits
        ]
        return self._ok(tool, results, started,
                        hint="distance-1 hits are the most directly affected")

    def hybrid_search(self, query: str, top_k: int = 10) -> dict:
        """RQ-01: one ranked list combining lexical (fulltext), dense
        (embedding), and graph (degree) signals — see
        `Neo4jRepository.hybrid_search` for the weighting."""
        tool = "hybrid_search"
        started = time.monotonic()
        try:
            matches = self._engine.hybrid_search(query, top_k=top_k)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "name": m.node.label,
                "kind": m.node.kind,
                "signature": m.node.signature,
                "source_file": m.node.source_file,
                "line_range": [m.node.line_start, m.node.line_end],
                "score": m.score,
                "lexical_score": m.lexical_score,
                "dense_score": m.dense_score,
                "graph_score": m.graph_score,
            }
            for m in matches
        ]
        if not results:
            return self._ok(
                tool, results, started,
                hint="no lexical/dense/graph matches for this query; try "
                     "search_symbol or semantic_search individually",
            )
        truncated = len(results) >= top_k
        return self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {top_k}" if truncated else None,
        )

    def entity_context(self, symbol: str) -> dict:
        """AI-02: one structured neighborhood block for `symbol` — node
        info (with IN-08 provenance), callers, callees, tests, and class
        hierarchy when applicable. Composes existing engine methods; see
        `QueryEngine.entity_context` for why there's no new Cypher here."""
        tool = "entity_context"
        started = time.monotonic()
        try:
            context = self._engine.entity_context(symbol)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not context:
            return self._ok(
                tool, [], started,
                hint=f"no symbol named '{symbol}'; try search_symbol first",
            )
        payload = {
            "node": _serialize.node_to_dict(context["node"]),
            "degree": context["degree"],
            "callers": [_serialize.edge_record_to_dict(r) for r in context["callers"]],
            "callees": [_serialize.edge_record_to_dict(r) for r in context["callees"]],
            "tests": [_serialize.edge_record_to_dict(r) for r in context["tests"]],
            "class_hierarchy": None,
        }
        hierarchy = context.get("class_hierarchy")
        if hierarchy:
            payload["class_hierarchy"] = {
                "node": _serialize.node_to_dict(hierarchy["node"]),
                "ancestors": [_serialize.node_to_dict(n) for n in hierarchy["ancestors"]],
                "interfaces": [_serialize.node_to_dict(n) for n in hierarchy["interfaces"]],
                "descendants": [_serialize.node_to_dict(n) for n in hierarchy["descendants"]],
                "implementers": [_serialize.node_to_dict(n) for n in hierarchy["implementers"]],
            }
        return self._ok(tool, [payload], started)

    def qa(self, question: str) -> dict:
        """AI-01: GraphRAG question-answering — hybrid_search retrieval,
        entity_context expansion, one LLM call, and a citation trail built
        entirely from the retrieval results (never from the LLM itself —
        see `cie.graphrag`'s module docstring). Runs the async pipeline
        via `asyncio.run` since every other `ToolService` method (and the
        `POST /tools/{tool}` dispatcher's `run_in_threadpool` wrapper
        around it) is synchronous — this is the same sync-bridge pattern
        `forge/watchdog/killbug_bridge.py` already uses for its own
        `asyncio.run(ask(...))` call from a plain function.

        `cie.graphrag` (which imports `core.llm`) is imported HERE, not
        at module level: `core.llm`'s own import chain reaches
        `core.graph.repository`, which imports `cie.factory`, which
        imports `cie.tools` (for `ToolService` itself) — a module-level
        import here is a genuine circular import (confirmed live, not
        hypothetical), not just this module importing `cie.graphrag`
        directly (which alone would ALSO cycle back through
        `cie.factory`).
        """
        from cie import graphrag as _graphrag

        tool = "qa"
        started = time.monotonic()
        try:
            result = asyncio.run(_graphrag.qa(question, engine=self._engine))
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        payload = {
            "answer": result.answer,
            "citations": [dataclasses.asdict(c) for c in result.citations],
            "retrieved_node_ids": list(result.retrieved_node_ids),
        }
        if not result.citations:
            return self._ok(
                tool, [payload], started,
                hint="no retrieved context grounded this answer; try "
                     "hybrid_search or search_symbol directly to check "
                     "what's indexed for this question",
            )
        return self._ok(tool, [payload], started)

    def class_hierarchy(self, class_name: str) -> dict:
        """DM-08: ancestors/interfaces/descendants/implementers of a class
        or interface, resolved from `extends`/`implements` edges."""
        tool = "class_hierarchy"
        started = time.monotonic()
        try:
            result = self._engine.class_hierarchy(class_name)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not result:
            return self._ok(
                tool, [], started,
                hint=f"no class or interface named '{class_name}' is indexed; "
                     "check with search_symbol or load/reindex the file first",
            )
        payload = {
            "node": _serialize.node_to_dict(result["node"]),
            "ancestors": [_serialize.node_to_dict(n) for n in result["ancestors"]],
            "interfaces": [_serialize.node_to_dict(n) for n in result["interfaces"]],
            "descendants": [_serialize.node_to_dict(n) for n in result["descendants"]],
            "implementers": [_serialize.node_to_dict(n) for n in result["implementers"]],
        }
        hint = (
            "ancestors/descendants are transitive over `extends`; "
            "interfaces/implementers are direct `implements` edges only "
            "(an ancestor's own interfaces are not unioned in)"
        )
        return self._ok(tool, [payload], started, hint=hint)

    def test_map(self, symbol: str, limit: int = 30) -> dict:
        """DM-14: which test(s) cover `symbol` — naming-convention +
        calls-edge-upgrade + `@patch`-target heuristics (see
        `cie.testlink`), not exhaustive."""
        tool = "test_map"
        started = time.monotonic()
        try:
            records = self._engine.test_map(symbol, limit=limit)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not records:
            return self._ok(
                tool, [], started,
                hint=f"no test found covering '{symbol}'; TESTS edges are "
                     "naming-convention/calls-edge/@patch-target heuristics, "
                     "not exhaustive - a non-conventionally-named test is missed",
            )
        results = [
            {
                "test": record.source_label,
                "implementation": record.target_label,
                "relation": record.edge.relation,
                "confidence": _confidence_value(record.edge.confidence),
            }
            for record in records
        ]
        truncated = len(results) >= limit
        return self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {limit}" if truncated else None,
        )

    # -- CI-01/02/03/04/05 clone detector -----------------------------------

    def clone_detect_run(self) -> dict:
        """Run the full clone-detection pass and write fresh
        `CloneCluster` nodes — an explicit, on-demand, whole-project
        analysis (see `cie.clone_detect`'s module docstring), NOT part of
        `reindex`/`reindex_file`'s per-file hot path. Safe to call
        repeatedly: each run REPLACES the prior run's clusters rather
        than accumulating them."""
        tool = "clone_detect_run"
        started = time.monotonic()
        try:
            summary = self._engine.clone_detect_run()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None
        if summary["clusters"] == 0:
            hint = (
                "no clone clusters found — either the project genuinely has "
                "no duplication above threshold, or NVIDIA_API_KEY wasn't "
                "set at load time (embedding-level detection needs it; "
                "token/AST-level detection does not)"
            )
        return self._ok(tool, [summary], started, hint=hint)

    def clone_clusters(self) -> dict:
        """Every clone cluster from the most recent `clone_detect_run`."""
        tool = "clone_clusters"
        started = time.monotonic()
        try:
            clusters = self._engine.clone_clusters()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "cluster_id": c["cluster"].id,
                "member_count": c["member_count"],
                "consolidation_target": c["consolidation_target"],
                "members": [
                    {"label": er.source_label, "id": er.edge.source}
                    for er in c["members"]
                ],
            }
            for c in clusters
        ]
        hint = None if results else "no clusters yet; run clone_detect_run first"
        return self._ok(tool, results, started, hint=hint)

    def clone_find(self, symbol: str) -> dict:
        """`symbol`'s clone cluster and clustermates, if any."""
        tool = "clone_find"
        started = time.monotonic()
        try:
            found = self._engine.clone_find(symbol)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not found:
            return self._ok(
                tool, [], started,
                hint=f"'{symbol}' has no clone cluster; either it's unique, "
                     "it's below the similarity threshold, or "
                     "clone_detect_run hasn't been run yet",
            )
        payload = {
            "node": _serialize.node_to_dict(found["node"]),
            "similarity": found["similarity"],
            "detected_by": found["detected_by"],
            "cluster_id": found["cluster"].id if found["cluster"] else None,
            "clustermates": [
                {"label": er.source_label, "id": er.edge.source}
                for er in found["clustermates"]
            ],
        }
        return self._ok(tool, [payload], started)

    # -- CI-06/07/08 static performance analyzer -----------------------------

    def performance_analyze_run(self) -> dict:
        """Run Big-O estimation (CI-06), anti-pattern detection (CI-07),
        and hot-path flagging (CI-08) — explicit, on-demand, whole-
        project, same as `clone_detect_run`. Safe to call repeatedly."""
        tool = "performance_analyze_run"
        started = time.monotonic()
        try:
            summary = self._engine.performance_analyze_run()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [summary], started)

    def performance_profile(self, symbol: str) -> dict:
        """`symbol`'s Big-O estimate, hot-path flag, and anti-pattern
        findings from the most recent `performance_analyze_run`."""
        tool = "performance_profile"
        started = time.monotonic()
        try:
            profile = self._engine.performance_profile(symbol)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not profile:
            return self._ok(
                tool, [], started,
                hint=f"no symbol named '{symbol}'; check with search_symbol",
            )
        payload = {
            "node": _serialize.node_to_dict(profile["node"]),
            "complexity_class": profile["complexity_class"],
            "hot_path": profile["hot_path"],
            "antipatterns": [
                _serialize.edge_record_to_dict(er) for er in profile["antipatterns"]
            ],
        }
        hint = None
        if not profile["complexity_class"]:
            hint = "no complexity_class yet; run performance_analyze_run first"
        return self._ok(tool, [payload], started, hint=hint)

    def antipattern_scan(self, file_glob: str = "") -> dict:
        """Every anti-pattern finding from the most recent
        `performance_analyze_run`, optionally narrowed to a file (plain
        substring match on `source_file`)."""
        tool = "antipattern_scan"
        started = time.monotonic()
        try:
            findings = self._engine.antipattern_scan(file_glob)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "symbol": f["node"].label,
                "source_file": f["node"].source_file,
                "line": f["node"].line_start,
                "pattern": f["pattern"],
                "severity": f["severity"],
                "detail": f["detail"],
            }
            for f in findings
        ]
        hint = None if results else "no findings; run performance_analyze_run first"
        return self._ok(tool, results, started, hint=hint)

    # -- CI-10/11/12 non-semantic drift detector -----------------------------

    def drift_detect_run(self) -> dict:
        """Run all three drift checks (CI-10/11/12) and write fresh
        `DriftFinding` nodes — explicit, on-demand, whole-project, same
        as `clone_detect_run`/`performance_analyze_run`. Pulls this
        ToolService's OWN already-bound project/root (it doesn't track a
        separate project string of its own — see `qa`'s docstring for
        the same reasoning) rather than requiring the caller to pass
        them again."""
        tool = "drift_detect_run"
        started = time.monotonic()
        try:
            from cie import drift_detect

            project = getattr(self._engine._repo, "_project", "") or ""  # noqa: SLF001
            tasks = (
                self._task_repo.list_all_for_project(project)
                if project and hasattr(self._task_repo, "list_all_for_project")
                else []
            )
            summary = drift_detect.analyze(
                self._engine._repo, self._root, tasks, project=project,  # noqa: SLF001
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None
        if not tasks and project:
            hint = (
                "requirement_gap found no tasks for this project — either "
                "none were pushed via tasks:push, or CI-10 genuinely has "
                "nothing to check; UNUSED_ROUTE/MISSING_ENDPOINT/"
                "CIRCULAR_DEPENDENCY are unaffected"
            )
        return self._ok(tool, [summary], started, hint=hint)

    def drift_report(self, drift_type: str = "") -> dict:
        """Every drift finding from the most recent `drift_detect_run`,
        optionally narrowed to one `drift_type`."""
        tool = "drift_report"
        started = time.monotonic()
        try:
            findings = self._engine.drift_report(drift_type)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "drift_type": f["drift_type"],
                "severity": f["severity"],
                "file": f["node"].source_file,
                "detail": f["detail"],
            }
            for f in findings
        ]
        hint = None if results else "no findings; run drift_detect_run first"
        return self._ok(tool, results, started, hint=hint)

    def architecture_check(self) -> dict:
        """CI-12 only, run live over the current graph state (no
        filesystem re-scan, no write) — a quick circular-dependency
        check without a full `drift_detect_run`."""
        tool = "architecture_check"
        started = time.monotonic()
        try:
            findings = self._engine.architecture_check()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if findings else "no circular dependencies found"
        return self._ok(tool, findings, started, hint=hint)

    # -- CI-19/20/21 metric computer ------------------------------------------

    def metrics(self) -> dict:
        """Compute + record clone_coverage_pct/drift_index/tech_debt_score
        from whatever section-13 analysis has already been run. Reading
        this WITHOUT having run `clone_detect_run`/`performance_analyze_run`/
        `drift_detect_run` first yields all-zero readings, not an error —
        see `cie.metrics`'s module docstring for why this pass can't tell
        "clean" from "never analyzed" on its own."""
        tool = "metrics"
        started = time.monotonic()
        try:
            result = self._engine.metrics()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def tech_debt_report(self, top_n: int = 20) -> dict:
        """Unified, prioritized tech-debt report: every clone cluster,
        anti-pattern, and drift finding, ranked by severity, plus the
        aggregate scores."""
        tool = "tech_debt_report"
        started = time.monotonic()
        try:
            report = self._engine.tech_debt_report(top_n=top_n)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [report], started)

    def metric_trend(self, metric_type: str = "", limit: int = 20) -> dict:
        """Historical metric readings (most recent first) — CI-21."""
        tool = "metric_trend"
        started = time.monotonic()
        try:
            snapshots = self._engine.metric_trend(metric_type, limit=limit)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "metric_type": s.metric_type, "value": s.value,
                "measured_at": s.measured_at, "detail": s.detail,
            }
            for s in snapshots
        ]
        hint = None if results else "no snapshots yet; run metrics first"
        return self._ok(tool, results, started, hint=hint)

    def resolve_api_route(self, path: str) -> dict:
        """Frontend API call path -> backend route(s) that serve it, with
        file/line and which service (be-v2 vs backend) actually handles it
        today per `frontend/vite.config.js`'s dev-proxy rules — the one
        HTTP-API-boundary question AST extraction alone can't answer (see
        `cie.api_routes` module docstring)."""
        tool = "resolve_api_route"
        started = time.monotonic()
        try:
            from cie import api_routes

            repo_root = api_routes.find_repo_root(self._root)
            results = api_routes.resolve_api_route(path, repo_root)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not results:
            return self._ok(
                tool, results, started,
                hint=f"no backend route matches '{path}'; check the path "
                     "template or that the route file was scanned "
                     "(be-v2/src and backend/src only)",
            )
        return self._ok(
            tool, results, started,
            hint="a result with is_forwarding_shim=true is backend/'s "
                 "bev2_proxy.py reverse-proxy handler, not the real "
                 "implementation — see its forwards_to for the be-v2 "
                 "route that actually runs",
        )

    def api_call_sites(self, route: str) -> dict:
        """Backend route (path template, e.g. '/api/tasks/{task_id}/kill',
        or a handler function name) -> frontend call site(s) that hit it.
        Reverse of `resolve_api_route`."""
        tool = "api_call_sites"
        started = time.monotonic()
        try:
            from cie import api_routes

            repo_root = api_routes.find_repo_root(self._root)
            results = api_routes.api_call_sites(route, repo_root)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not results:
            return self._ok(
                tool, results, started,
                hint=f"no frontend call site matches '{route}'; pass either "
                     "the exact backend path template or the handler "
                     "function name",
            )
        return self._ok(tool, results, started)

    # -- execution / indexing tools ------------------------------------------

    def run(self, cmd: str, timeout: int = 300) -> dict:
        """Sandboxed execution oracle (T1.4).

        ``ok`` mirrors the command's success (exit 0, no timeout), per the
        T1.4 example — a failing test suite yields ``ok: false`` WITH the
        captured output in ``results``.
        """
        tool = "run"
        started = time.monotonic()
        try:
            result = _runner.run_command(
                cmd,
                cwd=self._root,
                timeout=timeout,
                allowed_root=self._allowed_root,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "exit_code": result.exit_code,
                "output": result.output,
                "timed_out": result.timed_out,
                "output_truncated": result.output_truncated,
            }
        ]
        hint = None
        if result.timed_out:
            hint = f"timed out after {timeout}s; the process group was killed"
        envelope = self._ok(tool, results, started, hint=hint)
        envelope["ok"] = result.exit_code == 0 and not result.timed_out
        return envelope

    def write_file(self, path: str, content: str) -> dict:
        """Create or overwrite a file under the project root.

        Keeps the graph in sync in the SAME call — no separate
        ``reindex_file`` step for the caller to remember (see
        ``_sync_graph_after_write``). A reindex failure never fails the
        write itself; it only downgrades the envelope's hint.
        """
        tool = "write_file"
        started = time.monotonic()
        try:
            result = _edit.write_file(self._root, path, content)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        result, hint = self._sync_graph_after_write(path, result)
        return self._ok(tool, [result], started, hint=hint)

    def edit_file(
        self, path: str, old_string: str, new_string: str,
        replace_all: bool = False,
    ) -> dict:
        """Exact string replace in a file under the project root.

        Keeps the graph in sync in the SAME call (see ``write_file``).
        """
        tool = "edit_file"
        started = time.monotonic()
        try:
            result = _edit.edit_file(
                self._root, path, old_string, new_string, replace_all,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        result, hint = self._sync_graph_after_write(path, result)
        return self._ok(tool, [result], started, hint=hint)

    def delete_file(self, path: str) -> dict:
        """Delete a file under the project root, dropping its graph
        nodes/edges in the SAME call (mirrors write_file/edit_file)."""
        tool = "delete_file"
        started = time.monotonic()
        try:
            resolved = _view._jail(self._root, path)
            result = _edit.delete_file(self._root, path)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        self._invalidate_api_routes_cache()
        self._sync_heuristic_index(resolved)
        # Stale on purpose if left in place: a later reindex() comparing a
        # RECREATED file's hash against this deleted file's last-known
        # hash could match (same content) and wrongly skip re-indexing it
        # — but delete_file already dropped its graph nodes above, so
        # skipping would leave it permanently missing from the graph.
        self._indexed_hashes.pop(path, None)
        hint = None
        if extract.supported_suffix(resolved) is not None:
            try:
                self._engine._repo.reindex_file(str(resolved), extract.Extraction())  # noqa: SLF001
            except Exception as exc:  # noqa: BLE001
                hint = f"file deleted but graph cleanup failed: {exc}"
        return self._ok(tool, [result], started, hint=hint)

    def _sync_heuristic_index(self, resolved: Path) -> None:
        """Keep the lazy heuristic fallback index (see `_heuristic_fallback`)
        fresh after a write/edit/delete — but ONLY if it's already been
        built for this ToolService. Building it here just to update it
        would defeat the whole point of it being lazy (most ToolServices
        never touch it at all); a session that never fell back to it never
        pays this either, and one that already fell back once stays fresh
        without a full re-walk on every subsequent write.
        """
        if self._symbol_index is None:
            return
        # `resolved` always comes from `_view._jail`, which already
        # guarantees it's under `self._root` (raises otherwise) — no need
        # to re-check here.
        rel = str(resolved.relative_to(self._root))
        content = resolved.read_text(errors="replace") if resolved.is_file() else ""
        self._symbol_index.reindex_file(rel, content)

    def _invalidate_api_routes_cache(self) -> None:
        """Drop `cie.api_routes`'s per-repo-root route/call-site index after
        any write under the project root.

        That cache (resolve_api_route/api_call_sites — a regex scan of
        FastAPI route decorators + frontend fetch call sites, entirely
        separate from the Neo4j code graph) never invalidated itself: a
        forge session that generates or edits a route file and then later
        in the SAME run calls resolve_api_route/api_call_sites would get a
        stale answer from before its own edit. Cheap to over-invalidate
        (a write to an unrelated file just costs one extra rebuild on the
        NEXT resolve_api_route/api_call_sites call, not on this write) —
        far cheaper than a wrong answer.
        """
        from cie import api_routes

        api_routes.invalidate_cache(api_routes.find_repo_root(self._root))

    def _sync_graph_after_write(self, path: str, result: dict) -> tuple[dict, Optional[str]]:
        """Best-effort incremental reindex right after a write/edit landed
        on disk. Parses the just-written file and re-runs pass-2 call-edge
        resolution for it (see ``Repository.reindex_file``) so the graph
        never has a window where the caller has to remember a second step.

        Non-source files (no supported suffix) are left alone — nothing to
        index — and any indexing failure (bad parse, Neo4j unreachable)
        degrades to a hint rather than failing the write, which already
        landed on disk successfully by this point."""
        self._invalidate_api_routes_cache()
        resolved = _view._jail(self._root, path)
        self._sync_heuristic_index(resolved)
        if extract.supported_suffix(resolved) is None:
            return result, "not an indexable source file; graph unaffected"
        try:
            content = resolved.read_bytes()
            extraction = extract.extract_file(resolved)
            written = self._engine._repo.reindex_file(str(resolved), extraction)  # noqa: SLF001
            self._indexed_hashes[path] = hashlib.sha256(content).hexdigest()
        except Exception as exc:  # noqa: BLE001
            return result, f"write succeeded but graph reindex failed: {exc}"
        return {**result, "nodes_written": written}, None

    def reindex_file(self, path: str) -> dict:
        """Incrementally re-index one file.

        ``write_file``/``edit_file``/``delete_file`` already call this
        internally (see ``_sync_graph_after_write``) — this standalone
        entry point stays around for a mutation that landed OUTSIDE those
        three (e.g. a file written by ``run``, or a change made by a
        process that isn't going through this ToolService at all — a
        human editing on disk, another agent, a git merge). It's
        idempotent: re-indexing a file that's already fresh just repeats
        the same delete+reinsert with no observable effect.

        Parses the file with ``extract.extract_file`` and hands the
        Extraction to ``repo.reindex_file``, which re-runs pass-2
        ``callgraph.resolve_call_edges`` internally with read-after-write
        consistency.
        """
        tool = "reindex_file"
        started = time.monotonic()
        try:
            resolved = _view._jail(self._root, path)
            if not resolved.is_file():
                raise FileNotFoundError(
                    f"no such file under project root: {path}"
                )
            self._sync_heuristic_index(resolved)
            content = resolved.read_bytes()
            extraction = extract.extract_file(resolved)
            written = self._engine._repo.reindex_file(str(resolved), extraction)
            self._indexed_hashes[path] = hashlib.sha256(content).hexdigest()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"path": str(resolved), "nodes_written": written}], started,
            hint="graph is fresh for this file; callers of unchanged files "
                 "were re-resolved in the same call",
        )

    def reindex(self) -> dict:
        """Re-index every supported source file under the project root
        whose content has actually changed since it was last indexed.

        Calls this class's own `reindex_file` once per CHANGED file (a
        sha256 of the file's bytes is compared against `_indexed_hashes`,
        the same cache `reindex_file` itself keeps warm on every call —
        including the one `write_file`/`edit_file`/`delete_file` already
        make internally) — so a caller of `POST /tools/reindex` gets the
        exact same targeted, read-after-write-consistent path a single
        incremental update already uses, without paying to re-parse and
        re-embed every already-fresh file in the project.

        Confirmed live 2026-08-07: before this, every call re-indexed
        EVERY file unconditionally, no matter how many were already fresh
        from their own prior `reindex_file` call — on a forge run calling
        this (or, worse, the whole-project `reindex()`, not the per-file
        `reindex_file()`) after every single generated file, that made
        each successive write's graph-sync cost grow with total project
        size, the dominant cost of the whole run. `reindex_file` callers
        already avoided that; this makes `reindex()` itself safe to call
        repeatedly too, not just a one-time-per-process convention.

        Hashes are process-local (an in-memory dict, not persisted to the
        graph) — a fresh process always treats every file as unseen on its
        first `reindex()`/`reindex_file()` call, which is correct: the
        graph itself may be stale for reasons this process has no way to
        know about (another process's write, a git checkout) without
        actually re-parsing to find out. One file failing to index doesn't
        stop the rest.
        """
        tool = "reindex"
        started = time.monotonic()
        indexed = 0
        skipped = 0
        errors: list[dict] = []
        for suffix in extract._LANG_LOADERS:  # noqa: SLF001
            for f in sorted(self._root.rglob(f"*{suffix}")):
                if any(
                    part.startswith(".") or part in ("node_modules", "__pycache__", ".venv")
                    for part in f.parts
                ):
                    continue
                rel = str(f.relative_to(self._root))
                try:
                    current_hash = hashlib.sha256(f.read_bytes()).hexdigest()
                except OSError:
                    current_hash = None
                if current_hash is not None and self._indexed_hashes.get(rel) == current_hash:
                    skipped += 1
                    continue
                payload = self.reindex_file(rel)
                if payload.get("ok"):
                    indexed += 1
                else:
                    errors.append({
                        "path": rel,
                        "error": payload.get("error", {}).get("message", "unknown"),
                    })
        hint = None if not errors else f"{len(errors)} file(s) failed to index"
        envelope = self._ok(
            tool, [{"files_indexed": indexed, "files_skipped": skipped, "errors": errors}], started, hint=hint,
        )
        envelope["ok"] = not errors
        return envelope

    def start_watch(self, debounce: float = 0.5) -> dict:
        """Start an in-process filesystem watcher that incrementally
        reindexes changed source files under the project root.

        Used directly by the HTTP tool surface / `CieHTTPBackend`, and
        (since 2026-08-07) by forge's own in-process `CieBackend.
        start_watch()` too — that one used to spawn a SEPARATE `cie watch`
        subprocess instead, which survived independently of the request-
        handling process; it now just delegates here, trading that
        independence away for having no custom watch logic of its own (see
        `CieBackend.start_watch`'s docstring for the reasoning). Shares ONE
        handler implementation (`cie.tools.watch.build_reindex_handler`)
        with the standalone `cie watch` CLI subcommand so their debounce/
        reindex behavior can't drift apart — see that module's docstring.

        Idempotent: a second call while a watch is already running for this
        ToolService is a no-op. Never raises — a failed launch (`watchdog`
        not installed) degrades to an error envelope rather than crashing
        the caller.
        """
        tool = "start_watch"
        started = time.monotonic()
        if self._watch_observer is not None and self._watch_observer.is_alive():
            return self._ok(
                tool, [{"started": False}], started, hint="watch already running",
            )
        try:
            from watchdog.observers import Observer

            from cie.tools.watch import build_reindex_handler
        except ImportError:
            return self._err(
                tool, "internal", "watchdog not installed", started,
                hint="pip install watchdog to use start_watch",
            )
        # project="" matches every other write path in this class
        # (reindex_file/write_file/edit_file/delete_file never pass an
        # explicit project either) — see this module's own writes for the
        # established convention.
        handler = build_reindex_handler(self._engine._repo, "", debounce)  # noqa: SLF001
        observer = Observer()
        observer.schedule(handler, str(self._root), recursive=True)
        observer.start()
        self._watch_observer = observer
        return self._ok(tool, [{"started": True}], started)

    def stop_watch(self) -> dict:
        """Stop the watcher started by `start_watch`, if any. Safe to call
        even when `start_watch` was never called, or failed to launch."""
        tool = "stop_watch"
        started = time.monotonic()
        observer, self._watch_observer = self._watch_observer, None
        if observer is None or not observer.is_alive():
            return self._ok(
                tool, [{"stopped": False}], started, hint="watch was not running",
            )
        observer.stop()
        observer.join(timeout=5)
        return self._ok(tool, [{"stopped": True}], started)

    # -- git / task-graph tools ----------------------------------------------

    def blame_history(self, path: str, limit: int = 20) -> dict:
        """Git history for a path, joined with PRODUCED task artifacts (T2.4).

        Each git entry gains ``task: {"name", "userstory_id"}`` when a
        PRODUCED artifact for this path names the same commit (full/short
        sha prefix match in either direction).
        """
        tool = "blame_history"
        started = time.monotonic()
        try:
            entries = _blame.blame_history(self._root, path, limit)
            artifacts = self._task_repo.artifacts_for_path(path)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)

        def _matches(sha: str, commit_sha: str) -> bool:
            if not sha or not commit_sha:
                return False
            return sha.startswith(commit_sha) or commit_sha.startswith(sha)

        for entry in entries:
            for task_name, artifact in artifacts:
                commit_sha = getattr(artifact, "commit_sha", "")
                if not _matches(entry["commit_sha"], commit_sha):
                    continue
                task = self._task_repo.get_task(task_name)
                entry["task"] = {
                    "name": task_name,
                    "userstory_id": getattr(task, "userstory_id", "") if task else "",
                }
                break

        if not entries:
            return self._ok(
                tool, entries, started,
                hint=f"no git history for '{path}'; is the file committed "
                     "and is the project root a git repository?",
            )
        truncated = len(entries) >= limit
        return self._ok(
            tool, entries, started, truncated=truncated,
            hint=f"history capped at {limit} commits" if truncated else None,
        )

    # -- task store reads ------------------------------------------------------

    def list_pending_tasks(self) -> dict:
        """Pending atomic tasks for the forge loop (T3.1)."""
        tool = "list_pending_tasks"
        started = time.monotonic()
        try:
            tasks = self._task_repo.list_pending()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [_task_to_dict(t) for t in tasks]
        hint = None if results else "no pending tasks; push a batch with tasks:push"
        return self._ok(tool, results, started, hint=hint)

    def get_task(self, name: str) -> dict:
        """One atomic task by name (T3.1)."""
        tool = "get_task"
        started = time.monotonic()
        try:
            task = self._task_repo.get_task(name)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if task is None:
            return self._err(
                tool, "not_found", f"no task named '{name}'", started,
                hint="call list_pending_tasks to see available tasks",
            )
        return self._ok(tool, [_task_to_dict(task)], started)

    def task_dependency_closure(self, name: str) -> dict:
        """Transitive task-to-task DEPENDS_ON closure for topo sorting (T3.1)."""
        tool = "task_dependency_closure"
        started = time.monotonic()
        try:
            tasks = self._task_repo.get_dependent_tasks(name)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [_task_to_dict(t) for t in tasks]
        hint = (
            None
            if results
            else f"task '{name}' has no stored dependencies or is unknown; "
                 "check with get_task"
        )
        return self._ok(tool, results, started, hint=hint)
