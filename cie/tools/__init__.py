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
import difflib
import hashlib
import inspect
import logging
import time
from pathlib import Path
from typing import Any, Optional

from cie import ast_store as _ast_store
from cie import extract
from cie import file_index as _file_index
from cie import patch as _patch
from cie import serialize as _serialize
from cie.models import Confidence, Node, SymbolMatch

from cie.tools import blame as _blame
from cie.tools import edit as _edit
from cie.tools import runner as _runner
from cie.tools import view as _view
from cie.tools.heuristic import HeuristicToolSet
from cie.tools.imports import format_import
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
        project: str = "",
        max_file_size_bytes: int = _view.DEFAULT_MAX_FILE_SIZE_BYTES,
        hierarchy_repo: Optional[Any] = None,
    ) -> None:
        self._engine = engine
        self._task_repo = task_repo
        self._root = Path(root)
        self._allowed_root = (
            Path(allowed_root) if allowed_root is not None else self._root
        )
        # Threaded into view_file/write_file/edit_file/write_files_atomic
        # below (docs/plans/cie-standalone-any-project-plan.md Phase 4) —
        # per-instance so a caller that genuinely needs a bigger ceiling
        # (or a smaller one) doesn't have to pass it on every single call.
        self._max_file_size_bytes = max_file_size_bytes
        # This service's own project namespace — threaded into every
        # write_file/edit_file/delete_file/reindex_file's reindex_file()
        # call below, matching the project-scoping every READ query in
        # Neo4jRepository already applies via `_pw`/`_pw_where`. Reading
        # it off `engine`/`engine._repo` instead would only work for the
        # real Neo4jRepository (which happens to keep a private `_project`
        # of its own) and break the `Repository` Protocol's test doubles
        # (InMemoryRepository, the various fakes in tests/cie/) that have
        # no such attribute — this stays a plain constructor value instead.
        self._project = project
        # Populated by start_watch()/stopped by stop_watch(); an in-process
        # watchdog.observers.Observer, not the SEPARATE `cie watch`
        # subprocess forge's CieBackend spawns for its own in-process use
        # (forge/tools.py) — see cie.tools.watch's module docstring for why
        # both exist and share one handler implementation.
        self._watch_observer: Any = None
        # Populated by start_mock_server()/stopped by stop_mock_server()
        # (TE-05) — same "cached on this ToolService instance, alive for
        # as long as the caller keeps making calls against this project"
        # lifecycle as `_watch_observer` above.
        self._mock_server_handle: Any = None
        # The PRD-hierarchy store (roadmap R14): SQLiteHierarchyRepository
        # on the embedded backend (default `<root>/.cie/hierarchy.db`),
        # Neo4jHierarchyRepository through the factory's build path — or
        # None for callers that pass `hierarchy_tracking=False` (the
        # hierarchy tools then return an honest unavailable envelope,
        # mirroring the task_tracking=False / NullTaskRepository pattern;
        # fail-fast, never silently-empty).
        self._hierarchy_repo: Any = hierarchy_repo
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
        # Lazily built by _telemetry_bus() below — TE-09's real durable
        # pub/sub (see cie.apm's module docstring). Every ApmMetric this
        # service ingests is published on the "apm_metrics" channel
        # instead of being persisted via a bare merge_delta call, so (a)
        # the metric is also appended to the durable per-project telemetry
        # log (`Repository.publish_telemetry_event`, replay-able via
        # `TelemetryBus.replay`) and (b) any subscriber registered on this
        # bus (confidence scorer, drift detector, invariant monitor) is
        # notified synchronously. The ApmMetric `Node` write itself still
        # has to happen — apm_metrics()/performance_baseline() read it
        # back via `analysis_nodes("ApmMetric", ...)` — so a subscriber
        # that does exactly that merge_delta is registered the first time
        # this bus is built, instead of each call site doing it inline.
        self._telemetry: Optional[Any] = None

    # -- heuristic fallback ---------------------------------------------------

    def _heuristic_fallback(self) -> HeuristicToolSet:
        if self._heuristic is None:
            self._symbol_index = SymbolIndex(self._root)
            self._heuristic = HeuristicToolSet(self._root, self._symbol_index)
        return self._heuristic

    def _telemetry_bus(self) -> Any:
        """TE-09: this ToolService's `TelemetryBus` (see `_telemetry`'s
        field comment in `__init__`), built once and reused for the life
        of this instance. Registers the one subscriber every ApmMetric
        publish needs (persist the metric as a queryable `ApmMetric`
        `Node`) so ingestion call sites just `publish()` instead of each
        re-implementing that persistence inline."""
        if self._telemetry is None:
            from cie import apm as _apm

            project = self._canonical_project()
            bus = _apm.TelemetryBus(repo=self._engine._repo, project=project)  # noqa: SLF001

            def _persist_metric(metric: dict, _project: str = project) -> None:
                self._engine._repo.merge_delta([metric], [], project=_project)  # noqa: SLF001

            bus.subscribe("apm_metrics", _persist_metric)
            self._telemetry = bus
        return self._telemetry

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
        reason: Optional[str] = None,
    ) -> dict:
        """Build the SPEC §0 error envelope (see err_envelope for the
        machine-readable `reason` slug convention added in R5)."""
        error: dict = {"kind": kind, "message": message}
        if reason is not None:
            error["reason"] = reason
        return {
            "ok": False,
            "tool": tool,
            "error": error,
            "hint": hint,
            "elapsed_ms": _elapsed_ms(started),
        }

    def _guard(self, tool: str, started: float, exc: Exception) -> dict:
        """Convert tool exceptions into error envelopes.

        Failure classification candidates here carry a machine-readable
        `reason` slug (R5) so harnesses (`tool-test-lab/surface_conformance.py`,
        the reason-registry test) can assert against a stable string
        instead of parsing prose."""
        if isinstance(exc, FileNotFoundError):
            return self._err(tool, "not_found", str(exc), started,
                             hint="check the path with search_symbol or file_skeleton")
        if isinstance(exc, ValueError):
            return self._err(tool, "validation", str(exc), started,
                             hint="paths and working directories must stay "
                                  "inside the project root")
        if isinstance(exc, ModuleNotFoundError):
            # Optional backend absent (e.g. `core.llm` leftovers from the
            # protobox split) — a property of THIS installation, not a crash.
            missing = getattr(exc, "name", "") or str(exc)
            return self._err(
                tool, "unavailable", f"{type(exc).__name__}: {exc}", started,
                reason=f"OPTIONAL_BACKEND_MISSING:{(exc.name or 'unknown-module')}",
                hint=f"optional module '{missing}' is not installed in this standalone "
                     "package; this tool needs the full (protobox) environment to work",
            )
        try:
            from cie.decompose import DetectorUnavailable
        except Exception:  # pragma: no cover - import itself cannot be allowed to break the guard
            DetectorUnavailable = None  # type: ignore[assignment]
        if DetectorUnavailable is not None and isinstance(exc, DetectorUnavailable):
            # Optional decompose plugin absent (html walker / interactive-element
            # detector) — same "installation property" semantics.
            return self._err(
                tool, "unavailable", f"{type(exc).__name__}: {exc}", started,
                reason="HOST_PLUGIN_MISSING:decompose-detector",
                hint="register a decompose plugin entry-point (see the message) "
                     "or run inside the full be-v2 environment",
            )
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
            result = _view.view_file(
                self._root, path, start, end, skeleton,
                max_file_size_bytes=self._max_file_size_bytes,
            )
        except Exception as exc2:  # noqa: BLE001 - converted to envelope
            return self._guard(tool, started, exc2)
        window = result["window"]
        truncated = window["end"] < result["total_lines"]
        hint = result["hint"]
        if exc is not None:
            hint = self._degraded_hint(exc, hint)
        return self._ok(tool, [result], started, truncated=truncated, hint=hint)

    def get_meta(self, path: str) -> dict:
        """File meta from the AST mirror: file type, total lines, and
        the file's function signatures — served from the in-process AST
        snapshot, never a disk read.

        Sync contract (cie.ast_store's module docstring): every cie
        write path — write_file/write_files_atomic/edit_file/apply_patch
        via `_sync_graph_after_write`, delete_file, reindex_file,
        sync_ast_delta — refreshes the mirror in the SAME call, after the
        filesystem write has landed (apply_patch only reaches that hook
        after its post-write integrity re-hash, so a rolled-back patch
        never touches it). A change made OUTSIDE cie stays invisible to
        this tool until reindex_file/sync_ast_delta explicitly
        refreshes the mirror — that is what "cie is the single source
        where the file and its AST stay in sync" buys: no read can ever
        see a half-applied write. Signatures are Python-AST-exact;
        non-Python files honestly serve type/line count with a hint
        pointing at file_skeleton's heuristic tier.
        """
        tool = "get_meta"
        started = time.monotonic()
        try:
            resolved = _view._jail(self._root, path)
            entry = _ast_store.lookup_or_ingest(
                self._root, resolved, max_bytes=self._max_file_size_bytes)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        payload, truncated, hint = _ast_store.meta_payload(entry)
        return self._ok(tool, [payload], started, truncated=truncated, hint=hint)

    def get_function(self, path: str, signature: str,
                     start: int = 1, end: int = 0) -> dict:
        """One function's ACTUAL content, sliced from the AST snapshot —
        never a disk read (see `get_meta` for the sync contract).

        `signature` accepts whatever get_meta printed (full signature),
        a qualified name ("Class.method"), or a bare name — which must be
        unique in the file; ambiguity is an error listing candidates,
        never a guess.

        The 3,000-line-function answer: `start`/`end` are
        FUNCTION-RELATIVE (1-based, inclusive; `end=0` means WINDOW=100
        lines from `start`), and the payload ALWAYS carries the
        function's absolute file span, its length, and its
        nested-definition map — so a huge function is navigated (read
        the nested map, then request a narrow window) instead of dumped
        into the caller's context. `content` lines use view_file's exact
        ``{n:>5}\\t{line}`` format with ABSOLUTE file line numbers, so
        windows from the two tools cross-reference cleanly.
        """
        tool = "get_function"
        started = time.monotonic()
        try:
            resolved = _view._jail(self._root, path)
            entry = _ast_store.lookup_or_ingest(
                self._root, resolved, max_bytes=self._max_file_size_bytes)
            payload, truncated, hint = _ast_store.function_payload(
                entry, signature, start, end)
        except _ast_store.AstLookupError as exc:
            return self._err(tool, exc.kind, str(exc), started, hint=exc.hint)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [payload], started, truncated=truncated, hint=hint)

    # -- filesystem navigation (cie.file_index — pure index reads) -------

    def _ls_impl(self, tool: str, path: str, limit: int) -> dict:
        started = time.monotonic()
        try:
            resolved = _view._jail(self._root, path or ".")
            rel = _file_index.rel_under(self._root, resolved)
            payload, truncated, hint = _file_index.ls_payload(
                _file_index.for_root(self._root), rel, limit)
        except _file_index.FileIndexError as exc:
            return self._err(tool, exc.kind, str(exc), started, hint=exc.hint)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [payload], started, truncated=truncated, hint=hint)

    def ls(self, path: str = "", limit: int = _file_index.LS_CAP) -> dict:
        """One directory level from the in-process file index — dirs with
        files-within counts, files with their types. Never touches the
        filesystem: the index is built once per root per process
        (pruning cie.extract.EXCLUDED_DIRS — virtualenvs, node_modules,
        VCS internals, build outputs never enter it) and refreshed in
        the SAME call by every cie write (see cie.file_index's module
        docstring); external changes need reindex(). Empty dirs are
        invisible (files define shape). get_meta serves a file's
        contents-side meta; this serves the tree's shape.
        """
        return self._ls_impl("ls", path, limit)

    def dir(self, path: str = "", limit: int = _file_index.LS_CAP) -> dict:
        """Windows spelling of `ls` — the exact same payload. Registered
        as its own tool because a model reaching for `dir` would
        otherwise waste a turn on an unknown-tool error (the same class
        the shell-hallucination fixes target); both names, one index.
        """
        return self._ls_impl("dir", path, limit)

    def file_hierarchy(self, path: str = "", depth: int = 2,
                       max_entries: int = _file_index.TREE_ENTRY_CAP) -> dict:
        """ASCII tree of a subtree from the in-process file index —
        per-directory file counts, depth- and budget-limited with honest
        truncation notes. The orientation tool: an agent reads the
        shape first (file_hierarchy), then narrows (ls), then reads
        (get_meta/get_function/view_file) — instead of guessing paths.
        Same index, same sync contract as `ls`.
        """
        tool = "file_hierarchy"
        started = time.monotonic()
        try:
            resolved = _view._jail(self._root, path or ".")
            rel = _file_index.rel_under(self._root, resolved)
            payload, truncated, hint = _file_index.hierarchy_payload(
                _file_index.for_root(self._root), rel, depth, max_entries)
        except _file_index.FileIndexError as exc:
            return self._err(tool, exc.kind, str(exc), started, hint=exc.hint)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [payload], started, truncated=truncated, hint=hint)

    def file_names_like(self, pattern: str,
                        limit: int = _file_index.MATCH_CAP) -> dict:
        """Every indexed path matching an fnmatch glob ('*.py', '*test*',
        'pay*') — the whole-repo file-name search, over the same
        in-process index as `ls`. Sorted, capped, with the real count.
        """
        tool = "file_names_like"
        started = time.monotonic()
        try:
            payload, truncated, hint = _file_index.names_like_payload(
                _file_index.for_root(self._root), pattern, limit)
        except _file_index.FileIndexError as exc:
            return self._err(tool, exc.kind, str(exc), started, hint=exc.hint)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [payload], started, truncated=truncated, hint=hint)

    def path_prefix(self, prefix: str = "",
                    limit: int = _file_index.MATCH_CAP) -> dict:
        """Everything under a PARTIAL path — the half-remembered-path and
        typo-recovery tool, two binary searches away in the sorted index.
        Zero matches come back with the nearest existing neighbors as a
        'did you mean', so a wrong guess costs one turn, not a hunt.
        No jail prefix argument to normalize: it is a search key, and
        every result is an already-jailed relative path from the index.
        """
        tool = "path_prefix"
        started = time.monotonic()
        try:
            payload, truncated, hint = _file_index.prefix_payload(
                _file_index.for_root(self._root), prefix, limit)
        except _file_index.FileIndexError as exc:
            return self._err(tool, exc.kind, str(exc), started, hint=exc.hint)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [payload], started, truncated=truncated, hint=hint)

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

    def resolve_import(self, symbol: str, importing_file: str = "", language: str = "") -> dict:
        """Symbol/class name -> the exact import statement to write for it,
        resolved against the project's REAL current file layout instead of
        a guessed module path.

        Closes a recurring cross-task drift bug: one task creates a shared
        module (e.g. `backend/core/database.py`'s `get_db`/`Base`), a later
        task guesses a plausible-but-wrong import for it (e.g. `from
        backend.database import get_db`) instead of checking where the
        symbol actually lives. `forge/repair_agent.py`'s own failure-signal
        regexes can't reliably catch this after the fact — a plain Python
        `ModuleNotFoundError: No module named 'x.y'` names a dotted module,
        not a file path, and the traceback line pointing at the bad import
        (e.g. `features/auth/router.py:7`) falls outside the
        `src/`/`lib/`/`app/` prefixes `extract_signals` looks for — so it's
        cheaper to never guess the import in the first place. Prefer this
        over hand-deriving an import path for any symbol not defined in the
        file being written.

        `importing_file`: the file this import will be written into.
        Required for a correct RELATIVE path on JS/TS/TSX (Python's dotted
        path is project-root-relative regardless); also used to drop a
        same-file match (nothing to import — the symbol's already local)
        and to compute the relative specifier.

        `language`: optional filter ("python" | "typescript") for a symbol
        name that's ambiguous across stacks (e.g. both a Python and a TS
        file define something called `Booking`).
        """
        tool = "resolve_import"
        started = time.monotonic()
        search = self.search_symbol(symbol)
        if not search.get("ok", True):
            return search
        results, hint = format_import(
            search.get("results") or [], symbol, importing_file, language,
        )
        return self._ok(tool, results, started, hint=hint)

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

    def _call_resolution_summary(self, name: str) -> Optional[dict]:
        """R7: the persisted `CallResolutionStat` for `name`, as a summary
        dict — `{total_call_sites, unresolved_call_sites}` — or None when
        no stat was persisted (older index). Read through the repo's
        analysis_nodes (protocol method; both backends have it), never
        engine internals' shapes; best-effort, never load-bearing."""
        try:
            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "CallResolutionStat", project=self._canonical_project(),
            )
        except Exception:  # noqa: BLE001
            return None
        for n in nodes:
            if n.properties.get("name", n.label) == name:
                return {
                    "total_call_sites": n.properties.get("total_call_sites", 0),
                    "unresolved_call_sites": n.properties.get(
                        "unresolved_call_sites", 0
                    ),
                }
        return None

    def _edge_results(self, records: Any, direction: str,
                      provenance: str = "graph") -> list[dict]:
        """Shape EdgeRecords into caller/callee result dicts.

        Node file/signature are enriched best-effort via ``engine.get_node``
        (HEAD QueryEngine method) so results match the T1.3 shape. Every
        row carries `provenance` (R7): "graph" when it came from a
        persisted, confidence-tagged edge — "heuristic-name-match" when
        the heuristic index served the call, so a consumer can discount
        the fallback leg programmatically instead of via prose hints."""
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
                "provenance": provenance,
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
        """Blast radius: who calls ``symbol`` (T1.3).

        Rows carry `confidence` (edge-level EXTRACTED/INFERRED/AMBIGUOUS)
        and `provenance` (R7: "graph" vs "heuristic-name-match"); the
        envelope carries `resolution` — persisted per-name call-site
        totals so the known unresolved share (e.g. requests' 3-of-6 for
        `close()`) is visible in tool output, not only in the benchmark
        doc."""
        tool = "callers"
        started = time.monotonic()
        records, exc = self._try_graph(tool, self._engine.get_callers, symbol, limit)
        fallback = self._graph_result_or_heuristic(
            tool, started, records, exc,
            heuristic_call=lambda: self._heuristic_fallback().callers(symbol),
            empty_hint=HINT_EMPTY_CALLERS,
        )
        if fallback is not None:
            if fallback.get("ok") and isinstance(fallback.get("results"), list):
                for row in fallback["results"]:
                    if isinstance(row, dict):
                        row["provenance"] = "heuristic-name-match"
            return fallback
        results = self._edge_results(records, "caller")
        truncated = len(results) >= limit
        env = self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {limit}" if truncated else None,
        )
        resolution = self._call_resolution_summary(symbol)
        if resolution is not None:
            env["resolution"] = {
                **resolution,
                "resolved_edges": len(results),
            }
        return env

    def callees(self, symbol: str, limit: int = 30) -> dict:
        """Reverse localization: what ``symbol`` calls (T1.3).

        Rows carry `confidence` + `provenance` (R7); see `callers` for the
        envelope's `resolution` contract."""
        tool = "callees"
        started = time.monotonic()
        records, exc = self._try_graph(tool, self._engine.get_callees, symbol, limit)
        fallback = self._graph_result_or_heuristic(
            tool, started, records, exc,
            heuristic_call=lambda: self._heuristic_fallback().callees(symbol),
            empty_hint=HINT_EMPTY_CALLERS,
        )
        if fallback is not None:
            if fallback.get("ok") and isinstance(fallback.get("results"), list):
                for row in fallback["results"]:
                    if isinstance(row, dict):
                        row["provenance"] = "heuristic-name-match"
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
        tool = "qa"
        started = time.monotonic()
        try:
            # lazy: see docstring — imported here (inside the guard scope) so
            # the optional backend being absent degrades as `unavailable`, not
            # as a raw crash escaping to the MCP wrapper
            from cie import graphrag as _graphrag

            result = asyncio.run(_graphrag.qa(question, engine=self._engine))
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        payload = {
            "answer": result.answer,
            "citations": [dataclasses.asdict(c) for c in result.citations],
            "retrieved_node_ids": list(result.retrieved_node_ids),
            "faithfulness_score": result.faithfulness_score,
        }
        if not result.citations:
            return self._ok(
                tool, [payload], started,
                hint="no retrieved context grounded this answer; try "
                     "hybrid_search or search_symbol directly to check "
                     "what's indexed for this question",
            )
        hint = None
        if result.faithfulness_score is not None and result.faithfulness_score < 0.5:
            hint = (
                f"faithfulness_score {result.faithfulness_score} is low — the "
                "answer names identifiers this pass didn't retrieve; treat "
                "with extra scrutiny (see QG-07's caveats on this heuristic)"
            )
        return self._ok(tool, [payload], started, hint=hint)

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
                     "naming-convention/@patch-target/direct-call heuristics, "
                     "not exhaustive - a test that only reaches its target "
                     "through a helper is missed",
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

    # -- CI-15/16/17 runtime telemetry (section 13.4) ------------------------

    def actual_callers(self, symbol: str, limit: int = 30) -> dict:
        """CI-16: functions that ACTUALLY called `symbol` at runtime, per
        ingested OTel telemetry (see `POST /telemetry/otlp` — CI-15) —
        distinct from `callers`, which returns statically POSSIBLE
        callers from the AST call graph."""
        tool = "actual_callers"
        started = time.monotonic()
        try:
            records = self._engine.actual_callers(symbol, limit=limit)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not records:
            return self._ok(
                tool, [], started,
                hint=f"no runtime calls into '{symbol}' observed; either it's "
                     "never been called in production, or no OTel telemetry "
                     "has been ingested yet for this project (POST /telemetry/otlp)",
            )
        results = [
            {
                "caller": record.source_label, "callee": record.target_label,
                "call_count": record.edge.properties.get("call_count", 0),
                "avg_latency_ms": record.edge.properties.get("avg_latency_ms", 0.0),
                "error_rate": record.edge.properties.get("error_rate", 0.0),
                "last_seen": record.edge.properties.get("last_seen", ""),
            }
            for record in records
        ]
        truncated = len(results) >= limit
        return self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {limit}" if truncated else None,
        )

    def dead_code_confirm(self) -> dict:
        """CI-17: cross-reference the static call graph with ingested
        runtime telemetry. Classifies every FUNC/METHOD node as
        alive/potentially_dead/confirmed_dead/unknown — see
        `cie.telemetry.dead_code_confirm`'s docstring for the exact
        classification rules."""
        tool = "dead_code_confirm"
        started = time.monotonic()
        try:
            results = self._engine.dead_code_confirm()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        confirmed = sum(1 for r in results if r["classification"] == "confirmed_dead")
        hint = None
        if confirmed == 0:
            hint = "no confirmed-dead functions; either the graph is clean, " \
                   "or no OTel telemetry has been ingested yet (see 'unknown' entries)"
        return self._ok(tool, results, started, hint=hint)

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

    # -- RQ-04/AI-03 community-aware retrieval + global summarization -------

    def community_detect_run(self) -> dict:
        """Detect communities (label propagation) and patch `community`
        onto the graph's nodes — a prerequisite most of RQ-04's other
        pieces need (`community`/`get_community` were previously
        always-empty read paths with no write behind them)."""
        tool = "community_detect_run"
        started = time.monotonic()
        try:
            summary = self._engine.community_detect_run(project=self._canonical_project())
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if summary["communities"] else (
            "no communities found — the graph may have too few edges, "
            "or hasn't been loaded/reindexed yet"
        )
        return self._ok(tool, [summary], started, hint=hint)

    def community_summarize_run(self) -> dict:
        """One LLM-generated thematic summary per community from the
        most recent `community_detect_run`. Bridges the async pipeline
        via `asyncio.run`, same sync-bridge pattern `qa` uses (safe here
        for the same reason: `POST /tools/{tool}` already runs every
        handler inside `run_in_threadpool`, never the ASGI event loop
        thread)."""
        tool = "community_summarize_run"
        started = time.monotonic()
        try:
            summary = asyncio.run(
                self._engine.community_summarize_run(project=self._canonical_project())
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None
        if summary.get("summarized", 0) == 0:
            hint = "no summaries written; run community_detect_run first"
        elif summary.get("failed"):
            hint = f"{len(summary['failed'])} community summary(ies) failed to generate"
        return self._ok(tool, [summary], started, hint=hint)

    def community_search(self, query: str, top_k: int = 5) -> dict:
        """Thematic search over community summaries."""
        tool = "community_search"
        started = time.monotonic()
        try:
            results = self._engine.community_search(query, top_k=top_k)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if results else (
            "no matching community summaries; run community_detect_run "
            "then community_summarize_run first"
        )
        return self._ok(tool, results, started, hint=hint)

    # -- QG-01/02/03/04 quality/governance reports ---------------------------

    def accuracy_check(self, sample_size: int = 20) -> dict:
        """QG-01: re-parse a random sample of indexed symbols from disk
        and compare against the graph — flags a stale graph, doesn't
        fix it (re-run `reindex` for that)."""
        tool = "accuracy_check"
        started = time.monotonic()
        try:
            result = self._engine.accuracy_check(sample_size=sample_size)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None
        if result["mismatches"]:
            hint = f"{result['mismatches']} of {result['sampled']} sampled symbols are stale; run reindex"
        return self._ok(tool, [result], started, hint=hint)

    def freshness_report(self, stale_after_days: float = 7.0) -> dict:
        """QG-02: how much of the graph hasn't been re-extracted in
        `stale_after_days`."""
        tool = "freshness_report"
        started = time.monotonic()
        try:
            result = self._engine.freshness_report(stale_after_days=stale_after_days)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def comprehensiveness_report(self) -> dict:
        """QG-03: file/docstring/call-resolution coverage ratios, using
        this ToolService's own bound project root for the file-index
        ratio (no need for the caller to pass it)."""
        tool = "comprehensiveness_report"
        started = time.monotonic()
        try:
            result = self._engine.comprehensiveness_report(repo_root=str(self._root))
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def salience_report(self, top_n: int = 20) -> dict:
        """QG-04, READ-ONLY — lowest-salience symbols (pruning
        candidates only, never an actual prune)."""
        tool = "salience_report"
        started = time.monotonic()
        try:
            results = self._engine.salience_report(top_n=top_n)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, results, started,
            hint="lowest-salience symbols first; this is a report, not a "
                 "prune — nothing is deleted",
        )

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

    # -- Section 0 (Population & Real-Time Sync) ------------------------------

    def _canonical_project(self) -> str:
        return getattr(self._engine._repo, "_project", "") or ""  # noqa: SLF001

    def _speculative_engine(self):
        from cie import factory, sync

        canonical = self._canonical_project()
        if not canonical:
            raise ValueError(
                "the two-graph model requires a non-empty project; this "
                "ToolService is bound to the default/unnamed project"
            )
        return factory.get_engine(sync.speculative_project(canonical))

    def sync_quality_gate(self, path: str) -> dict:
        """PS-15: run the 4-stage quality gate (PS-04..PS-07) on one file
        and, if it passes stages 1/2/4, promote it into the canonical
        graph (PS-02) — SUSPECT/0.6 confidence if stage 3's linked tests
        failed, full confidence otherwise. Always updates the speculative
        graph first, unconditionally, even on a stage-1 rejection."""
        tool = "sync_quality_gate"
        started = time.monotonic()
        try:
            from cie import sync

            resolved = _view._jail(self._root, path)
            if not resolved.is_file():
                raise FileNotFoundError(f"no such file under project root: {path}")
            extraction = extract.extract_file(resolved)
            spec_engine = self._speculative_engine()
            project = self._canonical_project()
            layer_rules = sync.load_layer_rules(self._engine._repo, project)  # noqa: SLF001
            result = sync.run_quality_gate(
                spec_engine._repo, self._engine._repo,  # noqa: SLF001
                project, resolved, extraction,
                project_root=self._root, layer_rules=layer_rules or None,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        payload = {
            "path": result.path, "status": result.status,
            "confidence": result.confidence,
            "stages": [
                {
                    "stage": s.stage, "passed": s.passed, "score": s.score,
                    "violations": [
                        {"message": v.message, "detail": v.detail} for v in s.violations
                    ],
                }
                for s in result.stages
            ],
        }
        hint = None
        if result.status == "quarantined":
            hint = "blocked from promotion; see stages[].violations for why"
        elif result.status == "suspect":
            hint = "promoted with SUSPECT confidence (0.6) — linked test(s) failed"
        return self._ok(tool, [payload], started, hint=hint)

    def configure_layer_rules(self, layer_rules: dict) -> dict:
        """PS-07: persist per-project architectural layer-boundary rules
        (`{"layer_name": ["forbidden_substring", ...]}`) — every
        subsequent `sync_quality_gate` call auto-loads and enforces
        them, no need to pass them in by hand each time."""
        tool = "configure_layer_rules"
        started = time.monotonic()
        try:
            from cie import sync

            sync.store_layer_rules(self._engine._repo, self._canonical_project(), layer_rules)  # noqa: SLF001
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"layer_rules": layer_rules}], started)

    def get_layer_rules(self) -> dict:
        """PS-07: the currently-configured layer rules, `{}` if none set."""
        tool = "get_layer_rules"
        started = time.monotonic()
        try:
            from cie import sync

            layer_rules = sync.load_layer_rules(self._engine._repo, self._canonical_project())  # noqa: SLF001
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if layer_rules else "no layer rules configured; call configure_layer_rules first"
        return self._ok(tool, [{"layer_rules": layer_rules}], started, hint=hint)

    def install_git_hook(self, cie_url: str) -> dict:
        """PS-14 follow-up: installs a REAL `.git/hooks/post-commit`
        script into this ToolService's own project root, forwarding
        every commit to `POST /sync/event`. An existing hook is backed
        up, never silently overwritten (see `cie.sync.install_git_hook`'s
        own docstring). This is an explicit, caller-invoked action — no
        automatic installation happens on its own."""
        tool = "install_git_hook"
        started = time.monotonic()
        try:
            from cie import sync

            result = sync.install_git_hook(self._root, cie_url, self._canonical_project())
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = "existing hook backed up to post-commit.bak" if result["backed_up"] else None
        return self._ok(tool, [result], started, hint=hint)

    def sync_promote(self, commit_hash: str = "") -> dict:
        """PS-02: merge the speculative graph's current contents into
        canonical, stamped with `commit_hash`. Idempotent — safe to call
        again for the same commit."""
        tool = "sync_promote"
        started = time.monotonic()
        try:
            from cie import sync

            spec_engine = self._speculative_engine()
            written = sync.promote(
                spec_engine._repo, self._engine._repo,  # noqa: SLF001
                self._canonical_project(), commit_hash=commit_hash,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"nodes_written": written, "commit_hash": commit_hash}], started)

    def sync_revert(self, commit_hash: str) -> dict:
        """PS-12: soft-delete every canonical node stamped with
        `commit_hash` (status: DEPRECATED) — never a hard delete."""
        tool = "sync_revert"
        started = time.monotonic()
        try:
            from cie import sync

            count = sync.revert(self._engine._repo, self._canonical_project(), commit_hash)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if count else f"no canonical nodes stamped with commit {commit_hash!r}"
        return self._ok(tool, [{"deprecated_count": count}], started, hint=hint)

    def sync_ast_delta(self, path: str) -> dict:
        """PS-09/PS-10: symbol-level delta between what's currently
        stored for `path` and a fresh re-parse, plus any detected moves
        against the rest of the project's currently-removed symbols."""
        tool = "sync_ast_delta"
        started = time.monotonic()
        try:
            from cie import sync

            resolved = _view._jail(self._root, path)
            if not resolved.is_file():
                raise FileNotFoundError(f"no such file under project root: {path}")
            # AST mirror rides the same explicit-refresh path — this is
            # THE entry point for external changes (see get_meta's
            # docstring), so the mirror must refresh here too, not only
            # on cie's own writes.
            _ast_store.sync_path(self._root, resolved,
                                 max_bytes=self._max_file_size_bytes)
            _file_index.refresh_path(self._root, resolved, exists_on_disk=True)
            before = self._engine._repo.get_file_skeleton(str(resolved))  # noqa: SLF001
            fresh = extract.extract_file(resolved)
            after = [
                Node(id=n["id"], label=n.get("label", n["id"]), kind=n.get("kind", ""),
                     source_file=n.get("source_file", ""), signature=n.get("signature", ""),
                     docstring=n.get("docstring", ""), line_start=n.get("line_start", 0),
                     line_end=n.get("line_end", 0))
                for n in fresh.nodes
            ]
            delta = sync.ast_delta(before, after)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        payload = {
            "added": [n.label for n in delta["added"]],
            "removed": [n.label for n in delta["removed"]],
            "modified": [
                {"label": m["after"].label, "changed_fields": m["changed_fields"]}
                for m in delta["modified"]
            ],
            "unchanged_count": len(delta["unchanged"]),
        }
        return self._ok(tool, [payload], started)

    def sync_evict_speculative(self, ttl_seconds: int = 300) -> dict:
        """PS-01: delete speculative-graph nodes older than `ttl_seconds`."""
        tool = "sync_evict_speculative"
        started = time.monotonic()
        try:
            from cie import sync

            spec_engine = self._speculative_engine()
            spec_project = sync.speculative_project(self._canonical_project())
            count = sync.evict_stale_speculative(
                spec_engine._repo, spec_project, ttl_seconds=ttl_seconds,  # noqa: SLF001
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"evicted_count": count}], started)

    def sync_load_commit(self, repo_path: str, commit_hash: str) -> dict:
        """PS-16: idempotent, commit-linked batch population — checks out
        `commit_hash` into a temporary git worktree, extracts it, and
        loads it via `load_extraction`. `repo_path` is the git repository
        root (may differ from this ToolService's own project root)."""
        tool = "sync_load_commit"
        started = time.monotonic()
        try:
            from cie import sync

            written = sync.load_commit(
                self._engine._repo, Path(repo_path), commit_hash,  # noqa: SLF001
                project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"nodes_written": written, "commit_hash": commit_hash}], started,
        )

    # -- Section 1 (Core Data Model): DM-01/03/09/11/13 ------------------------

    def export_rdf(self, fmt: str = "turtle") -> dict:
        """DM-01: the whole project graph as RDF Turtle, for interop."""
        tool = "export_rdf"
        started = time.monotonic()
        try:
            text = self._engine.export_rdf(fmt=fmt)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"format": fmt, "rdf": text}], started)

    def related_edges(self, symbol: str) -> dict:
        """DM-03: `symbol`'s neighbors, each labeled with its relation's
        inverse (e.g. "calls" <-> "called_by")."""
        tool = "related_edges"
        started = time.monotonic()
        try:
            annotated = self._engine.related_edges(symbol)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "source": a["edge_record"].edge.source,
                "target": a["edge_record"].edge.target,
                "relation": a["edge_record"].edge.relation,
                "inverse": a["inverse"],
            }
            for a in annotated
        ]
        hint = None if results else f"'{symbol}' not found or has no neighbors"
        return self._ok(tool, results, started, hint=hint)

    def validate_property_constraints(self, constraints: dict) -> dict:
        """DM-03 follow-up: the "property constraints" half of the
        basic-ontology bucket (distinct from full OWL inference, still
        not attempted). `constraints` maps a property name to a boolean
        expression (`cie.invariants.evaluate_contract`'s restricted
        evaluator) checked against every node carrying that property."""
        tool = "validate_property_constraints"
        started = time.monotonic()
        try:
            from cie import data_model

            nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
            violations = data_model.validate_property_constraints(nodes, constraints)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, violations, started)

    def type_flow_run(self) -> dict:
        """DM-09: resolve TYPE_OF edges from every function/method's
        stored signature (return + parameter types)."""
        tool = "type_flow_run"
        started = time.monotonic()
        try:
            result = self._engine.type_flow_run(project=self._canonical_project())
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def type_flow(self, symbol: str) -> dict:
        """DM-09: parameter/return types `symbol` uses (or functions that
        use it as a type) — run `type_flow_run` first if this is empty."""
        tool = "type_flow"
        started = time.monotonic()
        try:
            records = self._engine.type_flow(symbol)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {"source": er.edge.source, "target": er.edge.target,
             "role": er.edge.properties.get("role", ""),
             "confidence": er.edge.confidence.value}
            for er in records
        ]
        hint = None if results else "no TYPE_OF edges yet; run type_flow_run first"
        return self._ok(tool, results, started, hint=hint)

    def dependency_graph_run(self) -> dict:
        """DM-11: parse `pyproject.toml`/`package.json` under this
        ToolService's project root into `PACKAGE` nodes, plus
        `CALLS_EXTERNAL` edges from every `FILE` node that actually
        imports one."""
        tool = "dependency_graph_run"
        started = time.monotonic()
        try:
            result = self._engine.dependency_graph_run(
                str(self._root), project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def dependency_graph(self) -> dict:
        """DM-11: every parsed external dependency (`PACKAGE` node)."""
        tool = "dependency_graph"
        started = time.monotonic()
        try:
            packages = self._engine.dependency_graph(project=self._canonical_project())
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {"name": p.label, "ecosystem": p.properties.get("ecosystem", ""),
             "section": p.properties.get("section", ""), "spec": p.properties.get("spec", "")}
            for p in packages
        ]
        hint = None if results else "no packages yet; run dependency_graph_run first"
        return self._ok(tool, results, started, hint=hint)

    def doc_graph_run(self) -> dict:
        """DM-13: parse markdown docs under this ToolService's project
        root into `DOCUMENT` nodes + `DESCRIBES` edges."""
        tool = "doc_graph_run"
        started = time.monotonic()
        try:
            result = self._engine.doc_graph_run(
                str(self._root), project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def doc_search(self, query: str) -> dict:
        """DM-13: `DOCUMENT` nodes whose label/excerpt matches `query`."""
        tool = "doc_search"
        started = time.monotonic()
        try:
            docs = self._engine.doc_search(query, project=self._canonical_project())
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [{"path": d.source_file, "label": d.label} for d in docs]
        hint = None if results else "no matching documents; run doc_graph_run first"
        return self._ok(tool, results, started, hint=hint)

    # -- Section 14 (Confidence Framework Integration) ------------------------

    def contracts_run(self, text: str) -> dict:
        """CF-01: LLM-extract formal contracts from requirement text,
        bind them to existing code symbols by name, and write them."""
        tool = "contracts_run"
        started = time.monotonic()
        try:
            from cie import contracts as _contracts

            extracted = asyncio.run(_contracts.extract_contracts(text))
            nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
            contract_nodes, edges = _contracts.bind_contracts_to_code(extracted, nodes)
            written = self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                "Contract", contract_nodes, edges, project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if edges else "contracts extracted but none bound to a known symbol by name"
        return self._ok(tool, [{"contracts_written": written, "bound_edges": len(edges)}], started, hint=hint)

    def contracts(self, scope: str = "", contract_type: str = "") -> dict:
        """CF-03: query already-extracted contracts by scope/type."""
        tool = "contracts"
        started = time.monotonic()
        try:
            from cie import contracts as _contracts

            all_nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "Contract", project=self._canonical_project(),
            )
            matches = _contracts.contracts_query(all_nodes, scope=scope, contract_type=contract_type)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {"id": n.id, "scope": n.properties.get("scope", ""),
             "contract_type": n.properties.get("contract_type", ""),
             "expression": n.properties.get("expression", "")}
            for n in matches
        ]
        hint = None if results else "no contracts yet; run contracts_run first"
        return self._ok(tool, results, started, hint=hint)

    def validate_types(self, domain_types: dict) -> dict:
        """CF-02 static half: flag parameters whose name implies a
        domain type (per `domain_types`, e.g. {"email": "Email"}) but
        whose annotation doesn't match."""
        tool = "validate_types"
        started = time.monotonic()
        try:
            from cie import contracts as _contracts

            nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
            violations = _contracts.validate_types(nodes, domain_types)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, violations, started)

    def inject_assertions(self, code: str, contract_specs: list) -> dict:
        """CF-02 assertion-injection half."""
        tool = "inject_assertions"
        started = time.monotonic()
        try:
            from cie import contracts as _contracts

            result = _contracts.inject_assertions(code, contract_specs)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"code": result}], started)

    def strip_assertions(self, code: str) -> dict:
        """CF-02 assertion-stripping half."""
        tool = "strip_assertions"
        started = time.monotonic()
        try:
            from cie import contracts as _contracts

            result = _contracts.strip_assertions(code)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"code": result}], started)

    def test_skeletons_run(self, specs: list) -> dict:
        """CF-04: generate test skeletons from a batch of
        `{criteria_text, target_label, test_type, spec_node_id}` specs
        and write them as `TestSkeleton` nodes + `TESTS` edges."""
        tool = "test_skeletons_run"
        started = time.monotonic()
        try:
            from cie import test_synthesis

            nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
            test_nodes, edges = test_synthesis.build_test_nodes(specs, nodes)
            written = self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                "TestSkeleton", test_nodes, edges, project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"skeletons_written": written, "bound_edges": len(edges)}], started)

    def test_skeletons(self, spec_node_id: str = "") -> dict:
        """CF-05: generated test skeletons, optionally filtered to one
        spec node."""
        tool = "test_skeletons"
        started = time.monotonic()
        try:
            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "TestSkeleton", project=self._canonical_project(),
            )
            if spec_node_id:
                nodes = [n for n in nodes if n.properties.get("spec_node_id") == spec_node_id]
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {"id": n.id, "test_type": n.properties.get("test_type", ""),
             "status": n.properties.get("status", ""), "skeleton_code": n.properties.get("skeleton_code", "")}
            for n in nodes
        ]
        hint = None if results else "no test skeletons yet; run test_skeletons_run first"
        return self._ok(tool, results, started, hint=hint)

    def test_coverage(self, spec_node_id: str) -> dict:
        """CF-05: is there a generated test for `spec_node_id`?"""
        tool = "test_coverage"
        started = time.monotonic()
        try:
            from cie import test_synthesis

            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "TestSkeleton", project=self._canonical_project(),
            )
            result = test_synthesis.test_coverage(spec_node_id, nodes)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def state_machine_run(self, text: str) -> dict:
        """CF-06: LLM-extract a finite state machine from requirement
        text and write it."""
        tool = "state_machine_run"
        started = time.monotonic()
        try:
            from cie import state_machine

            fsm = asyncio.run(state_machine.extract_state_machine(text))
            if fsm is None:
                return self._ok(
                    tool, [], started,
                    hint="no stateful entity found in the text; nothing extracted",
                )
            fsm_nodes, edges = state_machine.build_fsm_nodes(fsm)
            # fsm_nodes mixes THREE kinds (StateMachine/State/Transition) —
            # replace_analysis_nodes only deletes+recreates the ONE kind it's
            # called with, so each kind needs its own call (same grouping
            # decompose_page already does) or State/Transition rows from a
            # prior run never get cleaned up and could duplicate.
            project = self._canonical_project()
            by_kind: dict[str, list[dict]] = {}
            for n in fsm_nodes:
                by_kind.setdefault(n["kind"], []).append(n)
            written = 0
            for kind, kind_nodes in by_kind.items():
                kind_edges = [e for e in edges if any(n["id"] == e["source"] for n in kind_nodes)]
                written += self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                    kind, kind_nodes, kind_edges, project=project,
                )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"entity_name": fsm["entity_name"], "nodes_written": written}], started,
        )

    def state_machine(self, entity_name: str) -> dict:
        """CF-07: query a previously-extracted FSM by entity name."""
        tool = "state_machine"
        started = time.monotonic()
        try:
            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "StateMachine", project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        matches = [n for n in nodes if n.label == entity_name or entity_name in n.id]
        results = [
            {"id": n.id, "label": n.label, "kind": n.kind,
             "from_state": n.properties.get("from_state", ""), "to_state": n.properties.get("to_state", "")}
            for n in matches
        ]
        hint = None if results else f"no state machine found for '{entity_name}'; run state_machine_run first"
        return self._ok(tool, results, started, hint=hint)

    def fsm_validate(self, entity_name: str) -> dict:
        """CF-06/07: structurally validate a state machine's declared
        states against existing code symbol labels (see `cie.
        state_machine.validate_code_against_fsm`'s scoping)."""
        tool = "fsm_validate"
        started = time.monotonic()
        try:
            from cie import state_machine

            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "StateMachine", project=self._canonical_project(),
            )
            sm_nodes = [n for n in nodes if n.kind == "StateMachine" and n.label == entity_name]
            if not sm_nodes:
                return self._err(
                    tool, "not_found", f"no state machine named '{entity_name}'", started,
                    hint="run state_machine_run first",
                )
            state_nodes = [n for n in nodes if n.kind == "State" and n.id.startswith(sm_nodes[0].id + "::")]
            transition_nodes = [n for n in nodes if n.kind == "Transition" and n.id.startswith(sm_nodes[0].id + "::")]
            fsm = {
                "entity_name": entity_name,
                "states": [{"name": n.label} for n in state_nodes],
                "transitions": [
                    {"from_state": n.properties.get("from_state", ""), "to_state": n.properties.get("to_state", "")}
                    for n in transition_nodes
                ],
            }
            code_nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
            violations = state_machine.validate_code_against_fsm(fsm, code_nodes)
            unreachable = state_machine.find_unreachable_states(fsm)
            dead = state_machine.find_dead_states(fsm)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"violations": violations, "unreachable_states": unreachable, "dead_states": dead}], started,
        )

    def traceability_coverage(self) -> dict:
        """CF-08/09: fraction of code symbols tested/contracted."""
        tool = "traceability_coverage"
        started = time.monotonic()
        try:
            from cie import traceability

            nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            result = traceability.get_coverage(nodes, edges)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def traceability_orphans(self) -> dict:
        """CF-08/09: code symbols with no test and no contract."""
        tool = "traceability_orphans"
        started = time.monotonic()
        try:
            from cie import traceability

            nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            orphans = traceability.find_orphans(nodes, edges)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [{"id": n.id, "label": n.label, "source_file": n.source_file} for n in orphans]
        hint = None if results else "no orphans found (or graph is empty)"
        return self._ok(tool, results, started, hint=hint)

    def traceability_chain(self, symbol: str) -> dict:
        """CF-09: full test/contract chain for one code symbol."""
        tool = "traceability_chain"
        started = time.monotonic()
        try:
            from cie import traceability

            nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            result = traceability.traceability_chain(nodes, edges, symbol)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def prd_traceability_coverage(self) -> dict:
        """CF-08, PRD-hierarchy side: what fraction of file-bearing PRD
        hierarchy nodes (AtomicTasks) have a matching FILE node in the
        code graph. See `cie.traceability.prd_coverage`'s own docstring
        for why this is a separate function from the code-side
        `traceability_coverage`, not a merge of the two."""
        tool = "prd_traceability_coverage"
        started = time.monotonic()
        try:
            from cie import factory, traceability

            project = self._canonical_project()
            hierarchy_nodes = factory.get_hierarchy_repo(project).get_project_tree(project)
            code_nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
            result = traceability.prd_coverage(hierarchy_nodes, code_nodes)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if result["total_prd_file_tasks"] else "no file-bearing hierarchy nodes; push_hierarchy first"
        return self._ok(tool, [result], started, hint=hint)

    def prd_traceability_orphans(self) -> dict:
        """CF-08, PRD-hierarchy side: PRD tasks with a declared file
        that has no matching FILE node in the code graph yet."""
        tool = "prd_traceability_orphans"
        started = time.monotonic()
        try:
            from cie import factory, traceability

            project = self._canonical_project()
            hierarchy_nodes = factory.get_hierarchy_repo(project).get_project_tree(project)
            code_nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
            results = traceability.prd_orphans(hierarchy_nodes, code_nodes)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if results else "no PRD-side orphans found (or no hierarchy pushed yet)"
        return self._ok(tool, results, started, hint=hint)

    def prd_traceability_chain(self, task_id: str) -> dict:
        """CF-09, PRD-hierarchy side: task -> implemented_in (FILE
        node(s)) -> tested_by (TESTS edges on that file's symbols)."""
        tool = "prd_traceability_chain"
        started = time.monotonic()
        try:
            from cie import factory, traceability

            project = self._canonical_project()
            hierarchy_nodes = factory.get_hierarchy_repo(project).get_project_tree(project)
            code_nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            result = traceability.prd_traceability_chain(hierarchy_nodes, code_nodes, edges, task_id)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not result.get("found"):
            return self._err(
                tool, "not_found", f"no hierarchy node '{task_id}'", started,
                hint="call push_hierarchy first, or check the id with get_children",
            )
        return self._ok(tool, [result], started)

    def semantic_diff(self, spec_text: str, target_symbol: str) -> dict:
        """CF-10/11: prototype-level spec-vs-code contradiction check
        (see `cie.semantic_diff`'s own scoping — pattern-matching
        heuristic, not graph isomorphism)."""
        tool = "semantic_diff"
        started = time.monotonic()
        try:
            from cie import semantic_diff as _semantic_diff

            record = self._engine.get_node(target_symbol)
            if record is None:
                return self._err(
                    tool, "not_found", f"unknown symbol '{target_symbol}'", started,
                    hint="check the symbol name with search_symbol",
                )
            findings = _semantic_diff.detect_contradictions(spec_text, record.node)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if findings else "no known rule patterns matched (see module scope note — narrow heuristic)"
        return self._ok(tool, findings, started, hint=hint)

    def record_verdict(
        self, code_node_id: str, agent_name: str, verdict: str, confidence: float, reasoning: str,
    ) -> dict:
        """CF-12: store one agent's verdict on a code node."""
        tool = "record_verdict"
        started = time.monotonic()
        try:
            from cie import consensus as _consensus

            node, edge = _consensus.build_verdict_node(code_node_id, agent_name, verdict, confidence, reasoning)
            # merge_delta (MERGE-on-id), not replace_analysis_nodes — verdicts
            # accumulate one call at a time; replace_analysis_nodes would wipe
            # every OTHER already-recorded AgentVerdict on each new call.
            self._engine._repo.merge_delta(  # noqa: SLF001
                [node], [edge], project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"verdict_id": node["id"]}], started)

    def agent_verdicts(self, code_node_id: str) -> dict:
        """CF-12/14: every verdict recorded for `code_node_id`, plus the
        weighted consensus confidence."""
        tool = "agent_verdicts"
        started = time.monotonic()
        try:
            from cie import consensus as _consensus

            neighbors = self._engine._repo.get_neighbors(code_node_id, relation_filter="VERDICT_ON") or []  # noqa: SLF001
            all_verdicts = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "AgentVerdict", project=self._canonical_project(),
            )
            by_id = {n.id: n for n in all_verdicts}
            verdicts = [
                by_id[er.edge.source].properties for er in neighbors
                if er.edge.relation == "VERDICT_ON" and er.edge.source in by_id
            ]
            consensus_score = _consensus.consensus_confidence(verdicts)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if verdicts else "no verdicts recorded yet; call record_verdict"
        return self._ok(
            tool, [{"verdicts": verdicts, "consensus_confidence": consensus_score}], started, hint=hint,
        )

    def confidence_report(self) -> dict:
        """CF-15/16: multi-layer confidence report for this project."""
        tool = "confidence_report"
        started = time.monotonic()
        try:
            from cie import confidence as _confidence

            nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            verdict_nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "AgentVerdict", project=self._canonical_project(),
            )
            verdicts = [n.properties for n in verdict_nodes]
            report = _confidence.compute_confidence_report(nodes, edges, verdicts)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [report], started)

    def justification(self, code_node_id: str) -> dict:
        """CF-17/18: human-readable proof chain for why `code_node_id`
        exists (see `cie.justification`'s own scoping — no formal
        proof, a plain-English chain over already-existing traceability
        + consensus data)."""
        tool = "justification"
        started = time.monotonic()
        try:
            from cie import justification as _justification

            nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            verdict_nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "AgentVerdict", project=self._canonical_project(),
            )
            neighbors = self._engine._repo.get_neighbors(code_node_id, relation_filter="VERDICT_ON") or []  # noqa: SLF001
            by_id = {n.id: n for n in verdict_nodes}
            verdicts = [
                by_id[er.edge.source].properties for er in neighbors
                if er.edge.relation == "VERDICT_ON" and er.edge.source in by_id
            ]
            result = _justification.build_justification(code_node_id, nodes, edges, verdicts)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def check_invariant(self, contract_id: str, expression: str, code_node_id: str, state: dict) -> dict:
        """CF-19/20: evaluate one contract expression against a state
        snapshot; records + returns an `InvariantViolation` on failure
        (see `cie.invariants`'s own scoping — this is the checking
        primitive, not a continuous production monitor)."""
        tool = "check_invariant"
        started = time.monotonic()
        try:
            from cie import invariants as _invariants

            violation = _invariants.check_invariant(contract_id, expression, code_node_id, state)
            if violation is not None:
                existing = self._engine._repo.analysis_nodes(  # noqa: SLF001
                    "InvariantViolation", project=self._canonical_project(),
                )
                self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                    "InvariantViolation", [dict(n.properties, id=n.id, label=n.label, kind=n.kind,
                                                 source_file=n.source_file) for n in existing] + [violation],
                    [], project=self._canonical_project(),
                )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        holds = violation is None
        return self._ok(tool, [{"holds": holds, "violation": violation}], started)

    def invariant_violations(self) -> dict:
        """CF-20: every recorded invariant violation."""
        tool = "invariant_violations"
        started = time.monotonic()
        try:
            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "InvariantViolation", project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {"id": n.id, "contract_id": n.properties.get("contract_id", ""),
             "code_node_id": n.properties.get("code_node_id", ""),
             "severity": n.properties.get("severity", ""), "timestamp": n.properties.get("timestamp", "")}
            for n in nodes
        ]
        hint = None if results else "no violations recorded; call check_invariant"
        return self._ok(tool, results, started, hint=hint)

    def telemetry_to_spec(self, code_node_id: str) -> dict:
        """CF-21: trace a code node back to its contracts/tests (see
        `cie.invariants`'s own scoping — the "on a real runtime error"
        trigger needs telemetry ingestion this codebase doesn't have)."""
        tool = "telemetry_to_spec"
        started = time.monotonic()
        try:
            from cie import invariants as _invariants

            _nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            result = _invariants.telemetry_to_spec(code_node_id, edges)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    # -- Section 15 (Decomposition Engine) -------------------------------------

    def decompose_page(self, html: str, screen_id: str) -> dict:
        """DE-01/02/04: extract this screen's interactive elements +
        their derived task hints, plus the other pages it links to, and
        write everything to the canonical graph."""
        tool = "decompose_page"
        started = time.monotonic()
        try:
            from cie import decompose

            result = decompose.decompose_page(html, screen_id)
            by_kind: dict[str, list[dict]] = {}
            for n in result["all_nodes"]:
                by_kind.setdefault(n["kind"], []).append(n)
            written = 0
            for kind, kind_nodes in by_kind.items():
                kind_edges = [
                    e for e in result["all_edges"]
                    if any(n["id"] == e["source"] for n in kind_nodes)
                ]
                written += self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                    kind, kind_nodes, kind_edges, project=self._canonical_project(),
                )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{
                "screen_id": screen_id, "nodes_written": written,
                "element_count": len(result["elements"]),
                "derived_task_count": len(result["derived_tasks"]),
                "linked_page_count": len(result["linked_pages"]),
            }],
            started,
        )

    def page_tree(self) -> dict:
        """DE-05: every page, its interactive elements, and their
        derived task hints."""
        tool = "page_tree"
        started = time.monotonic()
        try:
            from cie import decompose

            project = self._canonical_project()
            pages = self._engine._repo.analysis_nodes("Page", project=project)  # noqa: SLF001
            elements = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "InteractiveElement", project=project,
            )
            _nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            tree = decompose.page_tree(pages, elements, edges)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if tree else "no pages yet; run decompose_page first"
        return self._ok(tool, tree, started, hint=hint)

    def promote_hint_to_task(self, hint_id: str, page_label: str, layer: str = "Frontend") -> dict:
        """DE-04 follow-up: promotes a `DerivedTaskHint` (the lightweight
        stand-in `decompose_page` creates) into a REAL, pushed
        `AtomicTask` — closes the gap the original DE-04 slice
        deliberately left open (`DerivedTaskHint`s never turning into
        anything pushable). Uses this ToolService's own `push_tasks`
        machinery, so the promoted task goes through the SAME validation
        every other pushed task does."""
        tool = "promote_hint_to_task"
        started = time.monotonic()
        try:
            from cie.decompose import promote_hint_to_task as _promote
            from cie.tasks import AtomicTask, AtomicTaskBatch

            project = self._canonical_project()
            hints = self._engine._repo.analysis_nodes("DerivedTaskHint", project=project)  # noqa: SLF001
            hint_node = next((h for h in hints if h.id == hint_id), None)
            if hint_node is None:
                return self._err(
                    tool, "not_found", f"no DerivedTaskHint '{hint_id}'", started,
                    hint="run decompose_page first, then check page_tree for hint ids",
                )
            task_dict = _promote(hint_node, page_label, layer=layer)
            batch = AtomicTaskBatch(tasks=[AtomicTask(**task_dict)])
            result = self._task_repo.push_tasks(batch, project=project)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"pushed": result.model_dump(mode="json")}], started)

    def element_coverage(self, page_id: str) -> dict:
        """DE-06: which of a page's interactive elements have at least
        one derived task, and which don't."""
        tool = "element_coverage"
        started = time.monotonic()
        try:
            from cie import decompose

            project = self._canonical_project()
            elements = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "InteractiveElement", project=project,
            )
            _nodes, edges = self._engine._repo.project_graph()  # noqa: SLF001
            result = decompose.element_coverage(page_id, elements, edges)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def implied_pages_run(self, known_labels: Optional[list] = None) -> dict:
        """DE-03: compare already-extracted `Page` nodes against
        `known_labels` and write `ImpliedPage` nodes for anything the UI
        implies but the PRD never named. `known_labels` is now OPTIONAL
        (follow-up, was required) — when omitted, auto-fetches PRD
        hierarchy node names via `cie.hierarchy.get_project_tree`, the
        same convenience `prd_traceability_coverage` already provides,
        so a caller doesn't have to separately look them up first."""
        tool = "implied_pages_run"
        started = time.monotonic()
        try:
            from cie import decompose

            project = self._canonical_project()
            if known_labels is None:
                from cie import factory

                hierarchy_nodes = factory.get_hierarchy_repo(project).get_project_tree(project)
                known_labels = [n.name for n in hierarchy_nodes]
            pages = self._engine._repo.analysis_nodes("Page", project=project)  # noqa: SLF001
            implied_nodes, edges = decompose.find_implied_pages(pages, known_labels)
            written = self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                "ImpliedPage", implied_nodes, edges, project=project,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"implied_pages_written": written}], started)

    def implied_pages(self) -> dict:
        """DE-07: pages implied by the UI but missing from the PRD (see
        `cie.decompose.find_implied_pages` — a `Page` becomes
        `ImpliedPage` when no PRD hierarchy node shares its label)."""
        tool = "implied_pages"
        started = time.monotonic()
        try:
            from cie import decompose

            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "ImpliedPage", project=self._canonical_project(),
            )
            results = decompose.implied_pages_report(nodes)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if results else "no implied pages found; run decompose_page then a find-implied-pages pass"
        return self._ok(tool, results, started, hint=hint)

    # -- Section 17 (System Intelligence & Subsystem Health) -------------------

    def subsystem_health(self) -> dict:
        """SI-02/SI-08: real-time status (green/yellow/red) for every
        registered subsystem, plus counts and a summary."""
        tool = "subsystem_health"
        started = time.monotonic()
        try:
            from cie import subsystems as _subsystems

            report = _subsystems.subsystem_health_report(self._engine._repo, self._canonical_project())  # noqa: SLF001
            dashboard = _subsystems.health_dashboard(report)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [dashboard], started)

    def subsystem_gaps(self) -> dict:
        """SI-03/SI-04: every empty/partial subsystem, why, what to do
        about it, and what's degraded/blocked downstream."""
        tool = "subsystem_gaps"
        started = time.monotonic()
        try:
            from cie import subsystems as _subsystems

            report = _subsystems.subsystem_health_report(self._engine._repo, self._canonical_project())  # noqa: SLF001
            statuses = {r["subsystem_id"]: r["status"] for r in report["subsystems"]}
            missing = _subsystems.missing_data_report(report)
            for entry in missing:
                entry["downstream_impact"] = _subsystems.downstream_impact(entry["subsystem_id"], statuses)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if missing else "every registered subsystem is populated"
        return self._ok(tool, missing, started, hint=hint)

    def subsystem_dependency_graph(self) -> dict:
        """SI-06: the subsystem registry itself, as a meta-graph
        (`Subsystem` nodes + `SUBSYSTEM_DEPENDS_ON`/`SUBSYSTEM_FEEDS`
        edges)."""
        tool = "subsystem_dependency_graph"
        started = time.monotonic()
        try:
            from cie import subsystems as _subsystems

            nodes, edges = _subsystems.dependency_graph_nodes()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"nodes": nodes, "edges": edges}], started)

    def subsystem_dependency_graph_run(self) -> dict:
        """SI-06 follow-up: actually WRITES the meta-graph
        (`dependency_graph_nodes()`'s rows were previously only
        returned for inspection) into a `{project}:_meta` namespace —
        scoped per underlying project (like PS-01's `:spec` suffix), not
        one shared global namespace, so a multi-tenant Neo4j instance
        can't have two projects' meta-graphs collide."""
        tool = "subsystem_dependency_graph_run"
        started = time.monotonic()
        try:
            from cie import subsystems as _subsystems

            meta_project = f"{self._canonical_project()}:_meta"
            nodes, edges = _subsystems.dependency_graph_nodes()
            written = self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                "Subsystem", nodes, edges, project=meta_project,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"nodes_written": written, "meta_project": meta_project}], started)

    def population_path(self, capability: str) -> dict:
        """SI-07: the minimum ordered set of subsystems that must be
        populated to enable `capability` (a subsystem id — see
        `subsystem_health`'s results for valid ids)."""
        tool = "population_path"
        started = time.monotonic()
        try:
            from cie import subsystems as _subsystems

            path = _subsystems.population_path(capability)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if path else f"unknown capability/subsystem id '{capability}'"
        return self._ok(tool, [{"capability": capability, "path": path}], started, hint=hint)

    # -- Section 16 (Autonomous Test Execution & APM) --------------------------

    def test_plan(self, prd_text: str = "", domain_types: Optional[dict] = None) -> dict:
        """TE-01: generate an exhaustive test plan covering every
        already-extracted `InteractiveElement`/`Contract`/`Transition`,
        every backend API route under this project's root, and
        (optionally) LLM-extracted PRD error scenarios (`prd_text`) and
        rule-based domain-type boundary cases (`domain_types`, the SAME
        `{param_name: type_name}` shape `validate_types` takes)."""
        tool = "test_plan"
        started = time.monotonic()
        try:
            from cie import test_orchestration as orch

            project = self._canonical_project()
            elements = self._engine._repo.analysis_nodes("InteractiveElement", project=project)  # noqa: SLF001
            contracts = self._engine._repo.analysis_nodes("Contract", project=project)  # noqa: SLF001
            transitions = self._engine._repo.analysis_nodes("Transition", project=project)  # noqa: SLF001
            nodes, edges = orch.generate_test_plan(elements, contracts, transitions, repo_root=self._root)

            if prd_text.strip():
                code_nodes, _edges = self._engine._repo.project_graph()  # noqa: SLF001
                scenarios = asyncio.run(orch.extract_error_scenarios(prd_text))
                scenario_nodes, scenario_edges = orch.build_error_scenario_tests(scenarios, code_nodes)
                nodes += scenario_nodes
                edges += scenario_edges

            if domain_types:
                boundary_nodes, boundary_edges = orch.generate_boundary_tests(domain_types)
                nodes += boundary_nodes
                edges += boundary_edges

            written = self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                "TestExecution", nodes, edges, project=project,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"test_executions_written": written}], started)

    def run_tests(self, test_type: str, target_files: list) -> dict:
        """TE-02: execute one layer for real (`unit`/`integration` only
        — every other layer reports `not_executed` with why; see
        `cie.test_orchestration`'s own scoping note). TE-08: for an
        EXECUTED layer, also automatically collects per-test latency via
        `cie.apm.collect_apm_from_pytest` (pytest's own `--junitxml`
        reporter — no caller-supplied numbers needed) and persists them
        as `ApmMetric` nodes."""
        tool = "run_tests"
        started = time.monotonic()
        try:
            from cie import apm as _apm
            from cie import test_orchestration as orch

            result = orch.run_test_layer(test_type, target_files, self._root)
            metrics_recorded = 0
            if result["status"] != "not_executed":
                metrics = _apm.collect_apm_from_pytest(target_files, self._root)
                if metrics:
                    bus = self._telemetry_bus()
                    for metric in metrics:
                        bus.publish("apm_metrics", metric)
                    metrics_recorded = len(metrics)
            result = {**result, "apm_metrics_recorded": metrics_recorded}
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def record_test_result(self, test_execution_id: str, status: str) -> dict:
        """Persist one `TestExecution`'s outcome (status one of
        pending/running/passed/failed/skipped) — the write side
        `test_results`/`coverage_gaps` read back from."""
        tool = "record_test_result"
        started = time.monotonic()
        try:
            count = self._engine._repo.update_node_properties(  # noqa: SLF001
                {test_execution_id: {"status": status}}, project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if not count:
            return self._err(
                tool, "not_found", f"unknown TestExecution '{test_execution_id}'", started,
                hint="run test_plan first",
            )
        return self._ok(tool, [{"test_execution_id": test_execution_id, "status": status}], started)

    def test_results(self) -> dict:
        """TE-03: aggregated multi-layer results, read from each
        `TestExecution`'s own (possibly still-`pending`) `status`."""
        tool = "test_results"
        started = time.monotonic()
        try:
            from cie import test_orchestration as orch

            executions = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "TestExecution", project=self._canonical_project(),
            )
            results = {e.id: {"status": e.properties.get("status", "pending")} for e in executions}
            report = orch.aggregate_results(executions, results)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [report], started)

    def coverage_gaps(self) -> dict:
        """TE-12: every `TestExecution` not currently `passed`."""
        tool = "coverage_gaps"
        started = time.monotonic()
        try:
            from cie import test_orchestration as orch

            executions = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "TestExecution", project=self._canonical_project(),
            )
            results = {e.id: {"status": e.properties.get("status", "pending")} for e in executions}
            gaps = orch.coverage_gaps(executions, results)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = None if gaps else "no gaps; run test_plan first if this looks wrong"
        return self._ok(tool, gaps, started, hint=hint)

    def nook_and_corner_test(self, write_files: bool = True) -> dict:
        """TE-13, ONE ROUND: generate a targeted test skeleton per
        current coverage gap (see `cie.test_orchestration`'s own scoping
        — a skip-marked skeleton can never auto-resolve a gap, so this
        is not a synchronous "repeat until threshold" loop; genuine
        gap-closing needs a human/agent to fill in a skeleton and call
        `record_test_result`). When `write_files` (default True), each
        skeleton is ALSO written to a real file under `tests/generated/`
        — not graph-only — so there's something to actually open."""
        tool = "nook_and_corner_test"
        started = time.monotonic()
        try:
            from cie import test_orchestration as orch

            project = self._canonical_project()
            executions = self._engine._repo.analysis_nodes("TestExecution", project=project)  # noqa: SLF001
            results = {e.id: {"status": e.properties.get("status", "pending")} for e in executions}
            gaps = orch.coverage_gaps(executions, results)
            skeleton_nodes, edges = orch.nook_and_corner_tests(gaps)
            written = self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                "TestSkeleton", skeleton_nodes, edges, project=project,
            )
            files_written: list[str] = []
            if write_files and skeleton_nodes:
                files_written = orch.write_skeleton_files(skeleton_nodes, self._root)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"gaps_found": len(gaps), "skeletons_written": written, "files_written": files_written}],
            started,
        )

    def unified_coverage_report(self) -> dict:
        """TE-14: every coverage dimension already computed elsewhere,
        assembled into one report (no new measurement)."""
        tool = "unified_coverage_report"
        started = time.monotonic()
        try:
            from cie import test_orchestration as orch

            project = self._canonical_project()
            traceability = self.traceability_coverage()
            regressions = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "PerformanceRegression", project=project,
            )
            report = orch.unified_coverage_report(
                traceability=traceability.get("results", [{}])[0] if traceability.get("ok") else None,
                performance_regression_count=len(regressions),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [report], started)

    def mock_registry_run(self) -> dict:
        """TE-04: scan every supported source file under this project's
        root for outbound calls to an external host, and write
        `MockEndpoint` nodes."""
        tool = "mock_registry_run"
        started = time.monotonic()
        try:
            from cie import extract as _extract
            from cie import mocking

            calls: list[dict] = []
            for suffix in _extract._LANG_LOADERS:  # noqa: SLF001
                for f in sorted(self._root.rglob(f"*{suffix}")):
                    if any(
                        part.startswith(".") or part in ("node_modules", "__pycache__", ".venv")
                        for part in f.parts
                    ):
                        continue
                    try:
                        text = f.read_text(errors="replace")
                    except OSError:
                        continue
                    calls.extend(mocking.scan_external_calls(text, str(f)))
            endpoints, edges = mocking.build_mock_endpoints(calls)
            written = self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                "MockEndpoint", endpoints, edges, project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"endpoints_written": written, "calls_found": len(calls)}], started,
        )

    def mock_registry(self) -> dict:
        """TE-04/06: every registered mock endpoint and its mock status."""
        tool = "mock_registry"
        started = time.monotonic()
        try:
            endpoints = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "MockEndpoint", project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {"id": e.id, "service_name": e.label, "endpoints": e.properties.get("endpoints", []),
             "mock_mode": e.properties.get("mock_mode", "")}
            for e in endpoints
        ]
        hint = None if results else "no mock endpoints yet; run mock_registry_run first"
        return self._ok(tool, results, started, hint=hint)

    def mock_coverage(self) -> dict:
        """TE-06: which discovered external services are mocked."""
        tool = "mock_coverage"
        started = time.monotonic()
        try:
            from cie import mocking

            endpoints = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "MockEndpoint", project=self._canonical_project(),
            )
            result = mocking.mock_coverage([e.label for e in endpoints], endpoints)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def start_mock_server(self, host: str = "127.0.0.1", port: int = 0) -> dict:
        """TE-05: start the real `cie.mock_server` app (stub mode) as a
        background thread, cached on this ToolService instance. Point a
        test environment's own HTTP client base-URL override at
        `base_url + "/mock/{service_name}/..."` — see `cie.mock_server`'s
        own docstring for why this is the pattern (explicit base-URL
        override, same as WireMock) rather than transparent network
        interception. Idempotent: a second call while one is already
        running for this ToolService returns the existing handle's
        `base_url` rather than starting a duplicate server."""
        tool = "start_mock_server"
        started = time.monotonic()
        try:
            from cie import mock_server

            if self._mock_server_handle is None:
                self._mock_server_handle = mock_server.start_mock_server(
                    project=self._canonical_project(), host=host, port=port,
                )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"base_url": self._mock_server_handle.base_url, "project": self._canonical_project()}], started,
        )

    def stop_mock_server(self) -> dict:
        """TE-05: stop this ToolService's running mock server, if any.
        A no-op (not an error) when none is running."""
        tool = "stop_mock_server"
        started = time.monotonic()
        try:
            was_running = self._mock_server_handle is not None
            if self._mock_server_handle is not None:
                self._mock_server_handle.stop()
                self._mock_server_handle = None
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"stopped": was_running}], started)

    def mock_violations(self, service_name: str, method: str, path: str) -> dict:
        """TE-07: validate one call against its `MockEndpoint`'s
        declared contract; persists + returns a `ContractViolation` on
        mismatch."""
        tool = "mock_violations"
        started = time.monotonic()
        try:
            from cie import mocking

            project = self._canonical_project()
            endpoints = self._engine._repo.analysis_nodes("MockEndpoint", project=project)  # noqa: SLF001
            matches = [e for e in endpoints if e.label == service_name]
            if not matches:
                return self._err(
                    tool, "not_found", f"no mock endpoint registered for '{service_name}'", started,
                    hint="run mock_registry_run first",
                )
            violation = mocking.validate_mock_call(matches[0], method, path)
            if violation is not None:
                self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                    "ContractViolation", [violation], [], project=project,
                )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"violation": violation}], started)

    def record_apm_metric(
        self, code_node_id: str, metric_type: str, value: float, percentile: str = "",
    ) -> dict:
        """TE-08: ingest one already-measured APM value."""
        tool = "record_apm_metric"
        started = time.monotonic()
        try:
            from cie import apm as _apm

            metric = _apm.build_apm_metric(code_node_id, metric_type, value, percentile=percentile)
            self._telemetry_bus().publish("apm_metrics", metric)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"metric_id": metric["id"]}], started)

    def apm_metrics(self, code_node_id: str = "", metric_type: str = "") -> dict:
        """TE-11: query already-ingested APM metrics."""
        tool = "apm_metrics"
        started = time.monotonic()
        try:
            from cie import apm as _apm

            all_metrics = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "ApmMetric", project=self._canonical_project(),
            )
            matches = _apm.filter_metrics(all_metrics, code_node_id=code_node_id, metric_type=metric_type)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {"code_node_id": m.properties.get("code_node_id", ""), "metric_type": m.properties.get("metric_type", ""),
             "value": m.properties.get("value"), "percentile": m.properties.get("percentile", "")}
            for m in matches
        ]
        return self._ok(tool, results, started)

    def performance_baseline(self, code_node_id: str) -> dict:
        """TE-10: compute (and persist) a p50/p95/p99 baseline for
        `code_node_id` from its already-ingested latency metrics."""
        tool = "performance_baseline"
        started = time.monotonic()
        try:
            from cie import apm as _apm

            project = self._canonical_project()
            all_metrics = self._engine._repo.analysis_nodes("ApmMetric", project=project)  # noqa: SLF001
            metric_dicts = [m.properties for m in all_metrics]
            baseline = _apm.compute_baseline(code_node_id, metric_dicts)
            if baseline is None:
                return self._err(
                    tool, "not_found", f"no latency metrics recorded for '{code_node_id}'", started,
                    hint="call record_apm_metric with metric_type='latency' first",
                )
            self._engine._repo.merge_delta([baseline], [], project=project)  # noqa: SLF001
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [baseline], started)

    def performance_regressions(self, code_node_id: str, current_p95: float) -> dict:
        """TE-10/11: compare `current_p95` against `code_node_id`'s
        stored baseline; persists + returns a `PerformanceRegression`
        when it exceeds the threshold."""
        tool = "performance_regressions"
        started = time.monotonic()
        try:
            from cie import apm as _apm

            project = self._canonical_project()
            baselines = self._engine._repo.analysis_nodes("PerformanceBaseline", project=project)  # noqa: SLF001
            matches = [b for b in baselines if b.properties.get("code_node_id") == code_node_id]
            if not matches:
                return self._err(
                    tool, "not_found", f"no baseline recorded for '{code_node_id}'", started,
                    hint="call performance_baseline first",
                )
            baseline_dict = {"id": matches[0].id, **matches[0].properties}
            regression = _apm.detect_regression(baseline_dict, current_p95)
            if regression is not None:
                self._engine._repo.replace_analysis_nodes(  # noqa: SLF001
                    "PerformanceRegression", [regression], [], project=project,
                )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [{"regression": regression}], started)

    def graph_diff(self, before_path: str, after_path: str) -> dict:
        """RQ-12: pure filesystem diff between two directory trees'
        extractions (e.g. two git worktrees, or a before/after of the
        same tree) — never touches the live graph. See `cie.graph_diff`'s
        module docstring for why this doesn't depend on DM-05 temporal
        versioning. Both paths are jailed under `allowed_root` (the same,
        broader jail the `run` tool uses — not `root`, since comparing
        two DIFFERENT trees is the whole point of this tool)."""
        tool = "graph_diff"
        started = time.monotonic()
        try:
            before_resolved = _view._jail(self._allowed_root, before_path)
            after_resolved = _view._jail(self._allowed_root, after_path)
            from cie import graph_diff as _graph_diff

            diff = _graph_diff.graph_diff(before_resolved, after_resolved)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        summary = _graph_diff.graph_diff_summary(diff)
        payload = {**diff, "summary": summary}
        hint = None
        if summary["added"] == summary["removed"] == summary["modified"] == 0:
            hint = "no differences found between the two trees"
        return self._ok(tool, [payload], started, hint=hint)

    # -- repair transaction layer (propose / apply / verify patches) ---------
    #
    # Three tools with three different jobs (see cie.patch's module
    # docstring): propose = "what should change?" (never mutates files),
    # apply = "can this change safely land?" (the ONLY file mutation, gated
    # + all-or-nothing), verify = "did it fix the thing, unbroken?" (never
    # mutates files). The component that proposes a fix never gets to
    # declare its own fix correct.

    def _resolve_file_node_ids(self, paths: set[str]) -> dict[str, str]:
        """rel path -> the FILE node id extract.py actually used for it
        (= str of the jail-resolved absolute path — see extract_file's
        `file_id = str(path)`). Only exact matches are returned; a changed
        file that was never indexed simply gets no PATCHES edge."""
        by_abs: dict[str, str] = {}
        try:
            for node in self._engine._repo.list_files():  # noqa: SLF001
                by_abs.setdefault(node.source_file, node.id)
        except Exception:  # noqa: BLE001 - graph down: no edges, never a crash
            return {}
        out: dict[str, str] = {}
        for rel in paths:
            try:
                resolved = _view._jail(self._root, rel)  # noqa: SLF001
            except ValueError:
                continue
            if resolved.is_file():
                out[rel] = out.get(rel) or by_abs.get(str(resolved), "")
        return {k: v for k, v in out.items() if v}

    def _repository_revision(self) -> str:
        """git rev-parse HEAD, best-effort — provenance only; a non-git
        project just reports empty and never fails a proposal over it."""
        try:
            result = _runner.run_command(
                "git rev-parse HEAD", cwd=self._root, timeout=10,
                allowed_root=self._allowed_root,
            )
            return result.output.strip().splitlines()[-1] \
                if result.output.strip() else ""
        except Exception:  # noqa: BLE001
            return ""

    def propose_patch(
        self,
        changes: list,
        test_id: str = "",
        root_cause: str = "",
        confidence: Optional[float] = None,
        evidence: Optional[list] = None,
        intended_behavior: str = "",
        expected_failure: str = "",
        after_patch: str = "",
        constraints: Optional[list] = None,
        allowed_files: Optional[list] = None,
        risk_level: str = "",
        agent: str = "",
        model: str = "",
    ) -> dict:
        """Construct a precise, reviewable repair proposal — and change
        NOTHING. Returns a full PatchPlan: one unified diff per change,
        blast-radius impact resolved from the graph, an auto-assessed risk
        level when none is supplied, provenance, and the file-content hash
        every change was proposed against (apply's Gate-1 anchor). The plan
        is persisted as an immutable PatchPlan node and becomes the input
        to apply_patch — which answers "can this safely land?" — then
        verify_patch, which answers "did it fix the thing?" on EVIDENCE.
        The proposer deliberately has no tool that both edits files and
        declares the result correct.
        """
        return self._propose_patch_impl(
            changes=changes, test_id=test_id, root_cause=root_cause,
            confidence=confidence, evidence=evidence,
            intended_behavior=intended_behavior,
            expected_failure=expected_failure, after_patch=after_patch,
            constraints=constraints, allowed_files=allowed_files,
            risk_level=risk_level, agent=agent, model=model,
        )

    def _propose_patch_impl(
        self, *, changes, test_id, root_cause, confidence, evidence,
        intended_behavior, expected_failure, after_patch, constraints,
        allowed_files, risk_level, agent, model,
    ) -> dict:
        tool = "propose_patch"
        started = time.monotonic()
        from datetime import datetime, timezone
        try:
            if not changes or not isinstance(changes, list):
                return self._err(
                    tool, "validation",
                    "changes must be a non-empty list of "
                    "[file, old_text, new_text] change objects", started,
                    hint="each change: {'file': 'src/x.py', 'old_text': '...', "
                         "'new_text': '...'} — pass operation='create' + "
                         "new_text for new files",
                )
            errors: list[str] = []
            for i, change in enumerate(changes):
                message = _patch.validate_change(change)
                if message:
                    errors.append(f"changes[{i}]: {message}")
            if errors:
                return self._err(
                    tool, "validation", "; ".join(errors), started,
                    hint="fix the change list and re-propose; the stale "
                         "proposal was rejected",
                )

            structured: list[dict] = []
            for change in changes:
                rel = change["file"]
                op = change.get("operation") or (
                    _patch.OP_EDIT if change.get("old_text") else _patch.OP_CREATE
                )
                old_text = change.get("old_text", "")
                new_text = change.get("new_text", "")
                if op == _patch.OP_EDIT:
                    try:
                        content = _view._jail(self._root, rel).read_text(  # noqa: SLF001
                            errors="replace"
                        )
                    except FileNotFoundError:
                        return self._err(
                            tool, "not_found",
                            f"edit change targets file {rel!r} which does not "
                            "exist (use operation='create' to add a new file)",
                            started,
                            hint="check the file exists with file_skeleton, or "
                                 "propose a create change",
                        )
                    context_problem = _patch.check_change_context(content, old_text)
                    if context_problem:
                        return self._err(tool, "validation", context_problem, started)
                    new_content = _patch.compute_new_content(content, old_text, new_text)
                    diff = "\n".join(
                        line for line in difflib.unified_diff(
                            content.splitlines(), new_content.splitlines(),
                            fromfile=f"a/{rel}", tofile=f"b/{rel}", lineterm="",
                        )
                    )
                    context_sha = _patch.content_sha(content)
                else:  # create
                    resolved = _view._jail(self._root, rel)  # noqa: SLF001
                    if resolved.is_file():
                        return self._err(
                            tool, "validation",
                            f"create change targets existing file {rel!r} — "
                            "use an edit change to modify it", started,
                            hint="propose an edit with old_text if you meant to "
                                 "modify, or a different filename for create",
                        )
                    diff = "\n".join(difflib.unified_diff(
                        [], new_text.splitlines(),
                        fromfile="/dev/null", tofile=f"b/{rel}", lineterm="",
                    ))
                    context_sha = _patch.content_sha("")
                structured.append({
                    "file": rel,
                    "symbol": change.get("symbol", ""),
                    "operation": op,
                    "old_text": old_text,
                    "new_text": new_text,
                    "diff": diff,
                    # Gate-1 anchor: apply re-validates against this state.
                    "context_sha256": context_sha,
                })

            # Scope gate (Gate 4) is enforced NOW too — fail at propose time
            # when the proposal declares an allowlist it immediately violates.
            scope_problem = _patch.scope_of(allowed_files, structured)
            if scope_problem:
                return self._err(tool, "validation", scope_problem, started)

            # Blast-radius enrichment — best-effort graph reads; failures
            # degrade to empty lists, never fail the proposal.
            affected_symbols: list[str] = []
            callers: list[dict] = []
            affected_tests: list[dict] = []
            for change in structured:
                if change["symbol"]:
                    affected_symbols.append(change["symbol"])
                call_env = self.callers(change.get("symbol") or change["file"])
                if call_env.get("ok"):
                    callers.extend(call_env.get("results", [])[:10])
                if change.get("symbol"):
                    tm_env = self.test_map(change["symbol"])
                    if tm_env.get("ok"):
                        affected_tests.extend(tm_env.get("results", []))

            risk = (
                {"level": risk_level, "reason": "caller-supplied", "source": "agent"}
                if risk_level
                else _patch.auto_risk(structured, len(callers))
            )
            created_at = datetime.now(timezone.utc).isoformat()
            plan = _patch.build_patch_plan(
                changes=structured, test_id=test_id, root_cause=root_cause,
                confidence=confidence, evidence=evidence,
                intended_behavior=intended_behavior,
                expected_failure=expected_failure, after_patch=after_patch,
                constraints=constraints, allowed_files=allowed_files,
                risk=risk,
                impact={
                    "affected_symbols": affected_symbols,
                    "affected_files": sorted({c["file"] for c in structured}),
                    "callers": callers,
                    "affected_tests": affected_tests,
                },
                agent=agent, model=model,
                repository_revision=self._repository_revision(),
                created_at=created_at,
            )
            node, edges = _patch.build_patch_node(
                plan, file_node_ids=self._resolve_file_node_ids(
                    {c["file"] for c in structured}),
                # test-node linking lands in a later pass; the plan already
                # carries test_id — no dangling edge for an unindexed test.
            )
            # A second open proposal for the same failing test is SUPERSEDED
            # by this one (patches are immutable; re-proposal is a NEW
            # lifecycle event, never a mutation of the old plan).
            superseded: list[str] = []
            if test_id:
                for other in self._engine._repo.analysis_nodes(  # noqa: SLF001
                    "PatchPlan", project=self._canonical_project(),
                ):
                    other_plan = other.properties.get("plan", {})
                    if (
                        other.properties.get("status") == _patch.STATUS_PROPOSED
                        and (other_plan.get("trigger", {}) or {}) \
                                .get("test_id", "") == test_id
                        and other.id != plan["patch_id"]
                    ):
                        superseded.append(other.id)
                if superseded:
                    self._engine._repo.update_node_properties(  # noqa: SLF001
                        {
                            pid: {
                                "status": _patch.STATUS_SUPERSEDED,
                                "superseded_by": plan["patch_id"],
                            }
                            for pid in superseded
                        },
                        project=self._canonical_project(),
                    )
            self._engine._repo.merge_delta(  # noqa: SLF001
                [node], edges, project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [plan], started,
            hint="nothing was modified — call apply_patch with this patch_id "
                 "to mutate the repo through the gate pipeline",
        )

    def apply_patch(self, patch_id: str) -> dict:
        """Take an already-created PatchPlan and safely mutate the
        repository — it NEVER decides what the fix should be, only whether
        THIS change can land. Five gates run before any byte is written:
        the patch must exist and still be PROPOSED (Gate 0 — patches are
        immutable and a REJECTED one is terminal); every target jailed
        under the project root (Gate 2); every edit's old_text must still
        match current content exactly once (Gate 1 — PATCH_CONTEXT_MISMATCH
        on a stale proposal, never a blind apply); the proposal's own file
        scope is respected (Gate 4); resulting content must parse (Gate 3,
        Python-exact) and respect the write ceiling. Then ONE all-or-
        nothing write (snapshot + rollback underneath), immediately
        integrity-checked by re-hashing every written file (Gate 5 — a
        post-write mismatch triggers full rollback), with the graph
        re-indexed in the SAME call. On gate failure the patch's status
        moves to REJECTED with the reason recorded — the fix is to re-
        propose against current content, never to retry the stale id.
        """
        tool = "apply_patch"
        started = time.monotonic()
        try:
            plans = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "PatchPlan", project=self._canonical_project(),
            )
            by_id = {n.id: n for n in plans}
            if patch_id not in by_id:
                return self._err(
                    tool, "not_found", f"unknown patch '{patch_id}'", started,
                    hint="list_patches to see every recorded proposal",
                )
            status = by_id[patch_id].properties.get(
                "status", _patch.STATUS_PROPOSED,
            )
            if status != _patch.STATUS_PROPOSED:
                if status == _patch.STATUS_APPLIED:
                    return self._err(
                        tool, "validation",
                        f"patch '{patch_id}' is already APPLIED — apply is "
                        "not re-runnable (patches are immutable); call "
                        "verify_patch to judge it", started,
                    )
                return self._err(
                    tool, "validation",
                    f"patch '{patch_id}' is {status} — apply rejected; "
                    + ("propose a new patch (a REJECTED patch is terminal; "
                       "its context was proven stale or out of scope)"
                       if status == _patch.STATUS_REJECTED
                       else "propose a fresh patch for the next repair step"),
                    started,
                )
            plan = by_id[patch_id].properties.get("plan", {})
            changes = plan.get("changes", [])

            # Gate 4 first: scope. Then per-change compute + validate.
            scope_problem = _patch.scope_of(
                (plan.get("intent", {}) or {}).get("allowed_files"), changes,
            )
            if scope_problem:
                self._reject_patch(patch_id, scope_problem)
                return self._err(tool, "validation", scope_problem, started)

            final_content: dict[str, str] = {}
            prior_content: dict[str, str] = {}
            for change in changes:
                rel = change["file"]
                resolved = _view._jail(self._root, rel)  # noqa: SLF001 — Gate 2
                if change["operation"] == _patch.OP_CREATE:
                    if resolved.is_file():
                        message = (
                            f"PATCH_CONTEXT_MISMATCH: create change targets "
                            f"{rel!r} which now exists"
                        )
                        self._reject_patch(patch_id, message)
                        return self._err(tool, "validation", message, started)
                    final_content[rel] = change["new_text"]
                    continue
                try:
                    current = resolved.read_text(errors="replace")
                except FileNotFoundError:
                    message = (
                        f"PATCH_CONTEXT_MISMATCH: {rel!r} no longer exists — "
                        "it was deleted or moved since the proposal"
                    )
                    self._reject_patch(patch_id, message)
                    return self._err(
                        tool, "not_found", message, started,
                        hint="re-propose; if the file is genuinely gone the "
                             "diagnosis is stale too",
                    )
                context_problem = _patch.check_change_context(
                    current, change["old_text"],
                )
                if context_problem:
                    self._reject_patch(patch_id, context_problem)
                    return self._err(tool, "validation", context_problem, started)
                # Gate 1 strictness: the content hash must match the state
                # the proposal was made against. old_text matching uniquely
                # is not enough on its own — any post-proposal edit shifts
                # the anchor, and a patch reviewed against revision A must
                # not silently land on a drifted revision B.
                if change.get("context_sha256") and \
                        _patch.content_sha(current) != change["context_sha256"]:
                    message = (
                        f"PATCH_CONTEXT_MISMATCH: {rel!r} changed since the "
                        "proposal was made (its content hash no longer "
                        "matches the proposal's context anchor — the change "
                        "was proposed against different file content). "
                        "Re-propose against current content; the stale "
                        "proposal was rejected."
                    )
                    self._reject_patch(patch_id, message)
                    return self._err(
                        tool, "validation", message, started,
                        hint="even a matching old_text cannot be trusted "
                             "when the file moved — diff the file and "
                             "re-propose against current content",
                    )
                prior_content[rel] = current
                final_content[rel] = _patch.compute_new_content(
                    current, change["old_text"], change["new_text"],
                )

            # Gate 3: syntax of the POST-change content, and write ceiling.
            for rel, content in final_content.items():
                syntax = _patch.syntax_check(rel, content)
                if syntax["valid"] is False:
                    message = (
                        f"patch invalid: {rel!r} would not parse after this "
                        f"change ({syntax['detail']})"
                    )
                    self._reject_patch(patch_id, message)
                    return self._err(
                        tool, "validation", message, started,
                        hint="fix the change (its unified diff is on the plan) "
                             "and re-propose",
                    )
                if len(content.encode()) > self._max_file_size_bytes:
                    message = f"{rel!r} result exceeds the write ceiling"
                    self._reject_patch(patch_id, message)
                    return self._err(
                        tool, "validation", message, started,
                        hint="split the change or raise max_file_size_bytes",
                    )

            # Gates all passed — atomic mutation (snapshot + rollback
            # underneath), then an immediate post-write integrity check.
            write_results = _edit.write_files_atomic(
                self._root, final_content,
                max_file_size_bytes=self._max_file_size_bytes,
            )
            integrity_failures: list[str] = []
            for rel, new_content in final_content.items():
                landed = _view._jail(self._root, rel).read_text(errors="replace")  # noqa: SLF001
                if landed != new_content:
                    integrity_failures.append(rel)
            if integrity_failures:
                for rel, before in prior_content.items():
                    resolved = _view._jail(self._root, rel)  # noqa: SLF001
                    resolved.write_text(before)
                message = (
                    f"post-write integrity check failed for {integrity_failures}; "
                    "all files rolled back to pre-patch content"
                )
                return self._err(
                    tool, "internal", message, started,
                    hint="filesystem state is exactly pre-patch; the patch "
                         "stays PROPOSED and may be re-applied after "
                         "investigating the writer that corrupted the write",
                )

            graph_hints: list[str] = []
            for rel in final_content:
                _, file_hint = self._sync_graph_after_write(
                    rel, {"path": rel, "applied": True},
                )
                if file_hint:
                    graph_hints.append(f"{rel}: {file_hint}")
            self._engine._repo.update_node_properties(  # noqa: SLF001
                {
                    patch_id: {
                        "status": _patch.STATUS_APPLIED,
                        "applied_at": time.strftime(
                            "%Y-%m-%dT%H:%M:%SZ", time.gmtime(),
                        ),
                        "applied_files": sorted(final_content),
                        "post_content_sha256": {
                            rel: _patch.content_sha(content)
                            for rel, content in final_content.items()
                        },
                    }
                },
                project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = "now judge it: verify_patch with this patch_id — apply does " \
               "NOT decide the fix is correct, only that it landed safely"
        if graph_hints:
            hint += "; " + "; ".join(graph_hints)
        return self._ok(
            tool,
            [{
                "patch_id": patch_id,
                "status": _patch.STATUS_APPLIED,
                "files": {
                    rel: {
                        "bytes_written": write_results[rel]["bytes_written"],
                        "created": write_results[rel]["created"],
                        "post_content_sha256": _patch.content_sha(
                            final_content[rel]),
                    }
                    for rel in sorted(final_content)
                },
            }],
            started, hint=hint,
        )

    def _reject_patch(self, patch_id: str, reason: str) -> None:
        """Record an apply-time rejection on the (still-immutable) plan."""
        try:
            self._engine._repo.update_node_properties(  # noqa: SLF001
                {
                    patch_id: {
                        "status": _patch.STATUS_REJECTED,
                        "rejected_reason": reason,
                    }
                },
                project=self._canonical_project(),
            )
        except Exception:  # noqa: BLE001 - bookkeeping never masks the error
            pass

    def verify_patch(
        self,
        patch_id: str,
        run_tests: bool = False,
        test_type: str = "unit",
        architecture_check: bool = False,
    ) -> dict:
        """Judge an APPLIED patch on evidence — never on the proposer's
        claim. Runs the checklist against the REPOSITORY, not against the
        plan: is the patch's content actually in each file (or the exact
        post-apply hash still intact), does everything still parse, do the
        imports the patch added resolve, which tests cover the changed
        symbols, does the declared counterfactual actually describe a real
        behavior change (old text gone, new text present) — then, only when
        the caller opts in, the affected tests are executed for real and
        the architecture-check pass runs. Never mutates the repository and
        never edits the plan's content — the only writes are the patch's
        own status (VERIFIED|FAILED) and an append-only verification
        history entry, so the FAILED → FAILED → VERIFIED chain needed for
        repair metrics accumulates. A patch that proposes-but-doesn't-apply
        is not verifiable; apply it first.
        """
        tool = "verify_patch"
        started = time.monotonic()
        try:
            plans = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "PatchPlan", project=self._canonical_project(),
            )
            by_id = {n.id: n for n in plans}
            if patch_id not in by_id:
                return self._err(
                    tool, "not_found", f"unknown patch '{patch_id}'", started,
                    hint="list_patches to see every recorded proposal",
                )
            node = by_id[patch_id]
            status = node.properties.get("status", _patch.STATUS_PROPOSED)
            if status in (_patch.STATUS_PROPOSED, _patch.STATUS_REJECTED,
                          _patch.STATUS_SUPERSEDED):
                return self._err(
                    tool, "validation",
                    f"patch '{patch_id}' is {status} — verify_patch judges an "
                    "APPLIED patch; call apply_patch first", started,
                )
            plan = node.properties.get("plan", {})
            changes = plan.get("changes", [])
            post_hashes = node.properties.get("post_content_sha256", {})
            checks: list[dict] = []

            # 1-2. content: every change's text is really in the file, and
            # the file is byte-identical to what apply recorded when strict.
            content_failures: list[str] = []
            changed_files = sorted({c["file"] for c in changes})
            for rel in changed_files:
                try:
                    current = _view._jail(self._root, rel).read_text(  # noqa: SLF001
                        errors="replace",
                    )
                except FileNotFoundError:
                    content_failures.append(
                        f"{rel}: does not exist (deleted after apply?)",
                    )
                    continue
                expected_hash = post_hashes.get(rel)
                if expected_hash:
                    if _patch.content_sha(current) == expected_hash:
                        continue
                    content_failures.append(
                        f"{rel}: content changed since apply (expected sha "
                        f"{expected_hash[:12]}… got {_patch.content_sha(current)[:12]}…)"
                    )
                    continue
                for change in [c for c in changes if c["file"] == rel]:
                    if change["operation"] == _patch.OP_CREATE:
                        continue
                    if change["new_text"] and change["new_text"] not in current:
                        content_failures.append(
                            f"{rel}: applied new_text is missing from the file",
                        )
            checks.append({
                "name": "patch_content_present",
                "status": "fail" if content_failures else "pass",
                "detail": "; ".join(content_failures)
                          if content_failures
                          else f"{len(changed_files)} file(s) match the "
                               "post-apply content",
            })

            # 3. syntax of every touched file as it stands NOW.
            syntax_failures: list[str] = []
            skipped_syntax: list[str] = []
            for rel in changed_files:
                try:
                    current = _view._jail(self._root, rel).read_text(  # noqa: SLF001
                        errors="replace",
                    )
                except FileNotFoundError:
                    continue
                syntax = _patch.syntax_check(rel, current)
                if syntax["valid"] is False:
                    syntax_failures.append(f"{rel}: {syntax['detail']}")
                elif syntax["valid"] is None:
                    skipped_syntax.append(rel)
            checks.append({
                "name": "syntax_valid",
                "status": (
                    "pass" if not syntax_failures
                    else "fail",
                ),
                "detail": (
                    "; ".join(syntax_failures) if syntax_failures
                    else "all touched files parse"
                    if not skipped_syntax
                    else "all touched files parse"
                    + f" (not exact-checked: {skipped_syntax})"
                ),
            })

            # 4. imports the patch added resolve somewhere.
            import_failures: list[str] = []
            import_checked = 0
            for change in changes:
                for line in _patch.net_new_imports(
                    change.get("old_text", ""), change.get("new_text", ""),
                ):
                    symbol = (
                        line[len("from "):].split()[0].strip()
                        if line.startswith("from ")
                        else line[len("import "):].split()[0].strip()
                    ).rstrip(",")
                    import_checked += 1
                    resolved = self.resolve_import(symbol, importing_file=change["file"])
                    if not (resolved.get("ok") and resolved.get("results")):
                        import_failures.append(
                            f"{change['file']}: '{line}' does not resolve "
                            "against the project index (third-party imports "
                            "are expected here — a warning, not a failure)",
                        )
            checks.append({
                "name": "imports_resolvable",
                "status": "pass" if not import_failures else "warn",
                "detail": (
                    f"{import_checked} net-new import(s) checked"
                    if import_checked and not import_failures
                    else "no net-new imports"
                    if not import_checked
                    else "; ".join(import_failures)
                ),
            })

            # 5. counterfactual: does the patch ACTUALLY change behavior in
            # the direction the diagnosis predicted (old behavior gone, new
            # behavior present) — not just "the suite is green".
            counterfactual = (plan.get("intent", {}) or {}).get(
                "counterfactual", {},
            ) or {}
            cf_failures: list[str] = []
            for change in changes:
                try:
                    current = _view._jail(self._root, change["file"]).read_text(  # noqa: SLF001
                        errors="replace",
                    )
                except FileNotFoundError:
                    cf_failures.append(f"{change['file']}: missing")
                    continue
                if change["operation"] == _patch.OP_EDIT:
                    if change["old_text"] in current:
                        cf_failures.append(
                            f"{change['file']}: pre-patch behavior text still "
                            "present — the change may not have taken effect",
                        )
                    if change["new_text"] not in current:
                        cf_failures.append(
                            f"{change['file']}: post-patch behavior text "
                            "absent — the change may have been reverted",
                        )
            checks.append({
                "name": "counterfactual_holds",
                "status": "fail" if cf_failures else "pass",
                "detail": (
                    "; ".join(cf_failures) if cf_failures
                    else "old behavior removed, new behavior present"
                    + (f"; declared: {counterfactual.get('after_patch', '')[:120]}"
                       if counterfactual.get("after_patch") else "")
                ),
            })

            # 6. regression surface: tests covering the changed symbols.
            # The failing test's own file (when the test symbol is indexed)
            # plus files of up to five TESTS-mapped regression tests — this
            # is the list executed_tests (opt-in below) runs.
            affected_tests = (plan.get("impact", {}) or {}).get(
                "affected_tests", [],
            )
            test_id = (plan.get("trigger", {}) or {}).get("test_id", "")
            test_files: list[str] = []

            def _file_of_symbol(symbol_name: str) -> str:
                env = self.search_symbol(symbol_name)
                for hit in env.get("results", []) if env.get("ok") else []:
                    f = hit.get("source_file", "")
                    if f and hit.get("name", "") == symbol_name:
                        return f
                return ""

            if test_id:
                f = _file_of_symbol(test_id)
                if f and f not in test_files:
                    test_files.append(f)
            for record in affected_tests[:5]:
                f = _file_of_symbol(record.get("test", ""))
                if f and f not in test_files:
                    test_files.append(f)
            test_files = sorted(dict.fromkeys(f for f in test_files if f))
            checks.append({
                "name": "regression_surface",
                "status": "pass" if (affected_tests or test_id) else "skip",
                "detail": (
                    f"failing test: {test_id}; "
                    f"{len(affected_tests)} mapped test(s): "
                    + ", ".join(
                        t.get("test", "") for t in affected_tests[:8]
                    )
                    if (affected_tests or test_id)
                    else "no failing test recorded and no TESTS-map edges "
                         "for the changed symbols — the plan's diagnosis "
                         "cannot be regression-checked"
                ),
            })

            # 7. (opt-in) execute for real.
            if run_tests:
                if not test_files:
                    checks.append({
                        "name": "executed_tests",
                        "status": "skip",
                        "detail": "no test files discovered to run "
                                  "(test files come from the failing-test "
                                  "lookup + TESTS edges; pass them yourself "
                                  "via run_tests when known)",
                    })
                else:
                    orch_result = self.run_tests(test_type, test_files)
                    checks.append({
                        "name": "executed_tests",
                        "status": (
                            "pass" if orch_result.get("ok")
                            and (orch_result.get("results") or [{}])[0]
                            .get("status") == "passed"
                            else "fail"
                        ),
                        "detail": str((orch_result.get("results") or [{}])[0]
                                      .get("status", "no result"))
                                  + ("; " + str(orch_result.get("hint", ""))
                                     if orch_result.get("hint") else ""),
                    })

            # 8. (opt-in) architecture check over the current graph state.
            if architecture_check:
                arch = self.architecture_check()
                violations = arch.get("results", []) if arch.get("ok") else []
                checks.append({
                    "name": "architecture_check",
                    "status": "pass" if not violations else "fail",
                    "detail": (
                        "no architectural violations"
                        if not violations
                        else f"{len(violations)} violation(s): "
                        + "; ".join(
                            str(v.get("rule", v.get("kind", v))[:80] if isinstance(v, dict) else str(v)[:80])
                            for v in violations[:5]
                        )
                    ),
                })

            hard_failures = [
                c for c in checks if c["status"] == "fail"
            ]
            verdict = _patch.STATUS_FAILED if hard_failures \
                else _patch.STATUS_VERIFIED
            history_entry = {
                "verified_at": time.strftime(
                    "%Y-%m-%dT%H:%M:%SZ", time.gmtime(),
                ),
                "status": verdict,
                "failed_checks": [c["name"] for c in hard_failures],
                "checks": checks,
            }
            prior_history = node.properties.get("verification_history", []) or []
            self._engine._repo.update_node_properties(  # noqa: SLF001
                {
                    patch_id: {
                        "status": verdict,
                        "last_verification": history_entry,
                        "verification_history": prior_history + [history_entry],
                    }
                },
                project=self._canonical_project(),
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = (
            f"verdict {verdict}; the report is envelope-only evidence — "
            "persist an INDEPENDENT verdict via record_verdict(patch_id, "
            "agent, 'pass'|'fail', confidence, reasoning) so the proposing "
            "agent never certifies its own fix"
        )
        return self._ok(
            tool,
            [{
                "patch_id": patch_id,
                "status": verdict,
                "checks": checks,
                "affected_tests": affected_tests,
                "test_files": test_files,
            }],
            started, hint=hint,
        )

    def get_patch(self, patch_id: str) -> dict:
        """One full PatchPlan by id — the immutable proposal snapshot plus
        its CURRENT lifecycle status and every audit property (apply/reject
        reasons, verification history)."""
        tool = "get_patch"
        started = time.monotonic()
        try:
            plans = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "PatchPlan", project=self._canonical_project(),
            )
            by_id = {n.id: n for n in plans}
            if patch_id not in by_id:
                return self._err(
                    tool, "not_found", f"unknown patch '{patch_id}'", started,
                    hint="list_patches to see every recorded proposal",
                )
            node = by_id[patch_id]
            result = {
                "patch_id": node.id,
                **(node.properties.get("plan", {}) or {}),
            }
            result["patch_id"] = node.id
            result["status"] = node.properties.get(
                "status", _patch.STATUS_PROPOSED,
            )
            for key in (
                "applied_at", "rejected_reason", "superseded_by",
                "last_verification", "verification_history",
            ):
                if key in node.properties:
                    result[key] = node.properties[key]
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, [result], started)

    def list_patches(self, status: str = "", limit: int = 20) -> dict:
        """Every recorded PatchPlan, newest first, with one-line summaries —
        the repair-history view the per-bug metrics (first-patch success
        rate, mean patches per bug, verdict-vs-outcome correlation) are
        computed from."""
        tool = "list_patches"
        started = time.monotonic()
        try:
            nodes = self._engine._repo.analysis_nodes(  # noqa: SLF001
                "PatchPlan", project=self._canonical_project(),
            )
            if status:
                nodes = [n for n in nodes
                         if n.properties.get("status") == status]
            nodes.sort(key=lambda n: n.properties.get("plan", {}).get(
                "created_at", "",
            ), reverse=True)
            total = len(nodes)
            nodes = nodes[: max(0, limit)]
            results = []
            for n in nodes:
                plan = n.properties.get("plan", {}) or {}
                results.append({
                    "patch_id": n.id,
                    "status": n.properties.get(
                        "status", _patch.STATUS_PROPOSED,
                    ),
                    "created_at": plan.get("created_at", ""),
                    "test_id": (plan.get("trigger", {}) or {}).get(
                        "test_id", "",
                    ),
                    "files": plan.get("impact", {}).get("affected_files", []),
                    "risk": (plan.get("risk", {}) or {}).get("level", ""),
                    "root_cause": (plan.get("diagnosis", {}) or {}).get(
                        "root_cause", "",
                    ),
                })
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, results, started, total=total, truncated=total > len(results),
            hint=(f"{total} patch(es) recorded; capped at {limit}"
                  if total > len(results) else None),
        )

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
            result = _edit.write_file(
                self._root, path, content,
                max_file_size_bytes=self._max_file_size_bytes,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        result, hint = self._sync_graph_after_write(path, result)
        return self._ok(tool, [result], started, hint=hint)

    def write_files_atomic(self, files: dict) -> dict:
        """Write every path in ``files`` (path -> content), all-or-nothing
        (docs/plans/cie-standalone-any-project-plan.md Phase 4) — snapshot
        + rollback on partial failure, see ``cie.tools.edit.
        write_files_atomic``'s own docstring for the exact guarantee.

        Keeps the graph in sync for every successfully-written file in the
        SAME call (see ``write_file``); a per-file reindex failure only
        downgrades that file's entry hint, same as ``write_file``'s own
        best-effort reindex — it does not trigger a filesystem rollback,
        since the write itself already landed and rolling back a
        successful disk write over a reindex-only failure would be worse
        than a stale graph entry.
        """
        tool = "write_files_atomic"
        started = time.monotonic()
        try:
            results = _edit.write_files_atomic(
                self._root, files, max_file_size_bytes=self._max_file_size_bytes,
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        failed_reindex: list[str] = []
        for path in list(results):
            results[path], file_hint = self._sync_graph_after_write(path, results[path])
            if file_hint:
                failed_reindex.append(f"{path}: {file_hint}")
        hint = "; ".join(failed_reindex) if failed_reindex else None
        return self._ok(tool, [{"files": results}], started, hint=hint)

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
                max_file_size_bytes=self._max_file_size_bytes,
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
        # AST mirror: the unlink and the mirror drop are one unit — a
        # later get_meta/get_function for this path must not serve a
        # deleted file's snapshot.
        _ast_store.drop(self._root, resolved)
        # and the file index forgets it in the same call — a later ls/
        # file_names_like must not serve a deleted file's path.
        _file_index.remove_path(self._root, resolved)
        # Stale on purpose if left in place: a later reindex() comparing a
        # RECREATED file's hash against this deleted file's last-known
        # hash could match (same content) and wrongly skip re-indexing it
        # — but delete_file already dropped its graph nodes above, so
        # skipping would leave it permanently missing from the graph.
        self._indexed_hashes.pop(path, None)
        hint = None
        if extract.supported_suffix(resolved) is not None:
            try:
                self._engine._repo.reindex_file(  # noqa: SLF001
                    str(resolved), extract.Extraction(), project=self._project,
                )
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

        That cache (a regex scan of FastAPI route decorators + frontend
        fetch call sites, entirely separate from the Neo4j code graph) is
        consumed by the internal analysis passes that still use it —
        `cie.drift_detect`'s API-contract-drift check (CI-11) and
        `cie.test_orchestration`'s API-endpoint test plan — neither of
        which is a user-facing tool anymore. It never invalidated itself:
        a run that generates or edits a route file and then later in the
        SAME run runs one of those passes would get a stale answer from
        before its own edit. Cheap to over-invalidate (a write to an
        unrelated file just costs one extra rebuild on the NEXT pass, not
        on this write) — far cheaper than a wrong answer.
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
        # AST mirror (cie.ast_store — get_meta/get_function's source of
        # truth): refreshed in the SAME call as the graph sync, only
        # after the write has landed on disk (and for apply_patch, after
        # its post-write integrity re-hash — a rolled-back patch never
        # reaches this line), so file system, graph, and AST move as one
        # unit per call. Never raises — a mirror hiccup degrades to a
        # hint exactly like the graph sync below it.
        ast_hint = _ast_store.sync_path(
            self._root, resolved, max_bytes=self._max_file_size_bytes)
        # File index rides the same hook: the just-written (or newly
        # created — apply_patch's new files land here too) path is in the
        # index the same call it landed on disk, so a following ls/
        # file_names_like can never miss a file cie itself just wrote.
        _file_index.add_path(self._root, resolved)
        if extract.supported_suffix(resolved) is None:
            return result, (ast_hint
                            or "not an indexable source file; graph unaffected")
        try:
            content = resolved.read_bytes()
            extraction = extract.extract_file(resolved)
            written = self._engine._repo.reindex_file(  # noqa: SLF001
                str(resolved), extraction, project=self._project,
            )
            self._indexed_hashes[path] = hashlib.sha256(content).hexdigest()
        except Exception as exc:  # noqa: BLE001
            return result, ("write succeeded but graph reindex failed: "
                            f"{exc}" + (f"; {ast_hint}" if ast_hint else ""))
        return {**result, "nodes_written": written}, ast_hint

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
            # AST mirror rides the SAME read: one disk read refreshes
            # graph and mirror together — this is the explicit-refresh
            # entry point for changes that landed OUTSIDE cie (see
            # cie.ast_store's module docstring), so the mirror must move
            # with it, not lag a write behind.
            mirror_hint = _ast_store.ingest_bytes(
                self._root, resolved, content,
                max_bytes=self._max_file_size_bytes)
            # File index: a file that appeared outside cie's write tools
            # becomes visible here — reindex_file is the per-file explicit
            # refresh for external changes.
            _file_index.refresh_path(self._root, resolved, exists_on_disk=True)
            extraction = extract.extract_file(resolved)
            written = self._engine._repo.reindex_file(  # noqa: SLF001
                str(resolved), extraction, project=self._project,
            )
            self._indexed_hashes[path] = hashlib.sha256(content).hexdigest()
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = ("graph is fresh for this file; callers of unchanged files "
                "were re-resolved in the same call")
        if mirror_hint:
            hint = f"{hint}; {mirror_hint}"
        return self._ok(
            tool, [{"path": str(resolved), "nodes_written": written}], started,
            hint=hint,
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
        # Full-walk explicit refresh for the FILE INDEX first — a new or
        # deleted external file is only visible to ls/file_hierarchy/
        # file_names_like/path_prefix after this, exactly as the graph's
        # own freshness story works here (the hash-based per-file
        # skipping below cannot see a file it never knew about).
        _file_index.rebuild(self._root)
        indexed = 0
        skipped = 0
        errors: list[dict] = []

        # Phase 1: find every CHANGED file and capture its before-state —
        # skeleton AND incoming `calls` edges — before touching ANY of
        # them. This must be a separate pass, not interleaved with the
        # reindex_file calls below: `reindex_file` deletes+recreates a
        # file's own nodes (and their outgoing edges) as part of ITS OWN
        # call, so a caller file processed earlier in the same batch can
        # already have lost the very edge a LATER file's move-detection
        # needs to reconnect, if capture happened only "just before" each
        # file's own turn instead of upfront for the whole batch. See
        # `cie.sync.apply_batch_move_detection`'s own docstring — a move
        # is inherently a two-file event, so per-file `reindex_file` can
        # never detect one on its own; this is the batch-level integration
        # point (PS-09/PS-10).
        changed_files: list[str] = []
        before_by_file: dict[str, list] = {}
        incoming_by_node_id: dict[str, list] = {}
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
                changed_files.append(rel)
                resolved = self._root / rel
                before = self._engine._repo.get_file_skeleton(str(resolved))  # noqa: SLF001
                before_by_file[rel] = before
                for node in before:
                    incoming_by_node_id[node.id] = self._engine._repo.get_neighbors(node.id) or []  # noqa: SLF001

        # Phase 2: actually reindex each changed file.
        after_by_file: dict[str, list] = {}
        for rel in changed_files:
            payload = self.reindex_file(rel)
            if payload.get("ok"):
                indexed += 1
                resolved = self._root / rel
                after_by_file[rel] = self._engine._repo.get_file_skeleton(str(resolved))  # noqa: SLF001
            else:
                errors.append({
                    "path": rel,
                    "error": payload.get("error", {}).get("message", "unknown"),
                })

        # Phase 3: batch-wide move detection now that every file's final
        # state is known.
        move_result = {"moves_detected": 0, "moves": [], "incoming_edges_reconnected": 0}
        if len(after_by_file) >= 2:
            from cie import sync as _sync

            try:
                move_result = _sync.apply_batch_move_detection(
                    self._engine._repo, before_by_file, after_by_file,  # noqa: SLF001
                    incoming_by_node_id, project=self._canonical_project(),
                )
            except Exception:  # noqa: BLE001 - move detection is advisory, never blocks reindex
                logger.warning("reindex: batch move detection failed", exc_info=True)
        hint = None if not errors else f"{len(errors)} file(s) failed to index"
        envelope = self._ok(
            tool, [{
                "files_indexed": indexed, "files_skipped": skipped, "errors": errors,
                "moves_detected": move_result["moves_detected"],
                "incoming_edges_reconnected": move_result["incoming_edges_reconnected"],
            }],
            started, hint=hint,
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
        # 2026-08-14 fix (RF6): this comment's claim was factually wrong —
        # reindex_file/write_file/edit_file (via _sync_graph_after_write)
        # DO pass project=self._project explicitly. Use the same value here
        # instead of a hardcoded "" so filesystem-watch-triggered reindexes
        # land in the same project scope as every other write path.
        handler = build_reindex_handler(self._engine._repo, self._project, debounce)  # noqa: SLF001
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

    # -- task/QA write-back — promoted from HTTP-only alias handlers ------
    # (roadmap R1: these were bespoke `cie.routes._tool_*` handlers, which
    # made the task/QA layer — the README's headline differentiator —
    # read-only on the default `cie-mcp --embedded` install because the
    # MCP server introspects only ToolService. Same kwargs, same envelope
    # shapes, same hints as the HTTP handlers they replace; the remaining
    # HTTP-only helpers (get_coverage/coverage_report/coverage_trend/
    # validate_*/push_hierarchy/get_children/get_lineage/health/
    # schema_version) are documented in
    # tests/test_tool_surface_invariants.py's HTTP_ONLY_HELPERS.)

    def push_tasks(self, tasks: Optional[list] = None) -> dict:
        """Push a batch of atomic tasks — validate + partial-accept (T3.1).

        Accepts the AtomicTaskBatch shape ({tasks: [...]}, each task the
        schemas/atomic_task.schema.json shape); idempotent per task name.
        Contradiction-free but invalid batches are PARTIALLY accepted:
        valid tasks land, invalid ones come back rejected with per-task
        reasons (e.g. a dev task missing its test_triad), never dropped
        silently."""
        tool = "push_tasks"
        started = time.monotonic()
        try:
            from cie.tasks import AtomicTaskBatch

            batch = AtomicTaskBatch.model_validate({"tasks": tasks or []})
            result = self._task_repo.push_tasks(batch, project=self._project)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = (
            f"{len(result.rejected)} task(s) rejected; fix the reasons and "
            "re-push (push is idempotent per task name)"
            if result.rejected
            else None
        )
        return self._ok(tool, result.model_dump(mode="json"), started, hint=hint)

    def set_task_status(
        self, name: str = "", status: str = "", updated_at: str = ""
    ) -> dict:
        """Task lifecycle write-back (T3.2): set one task's status.

        Valid statuses are the TaskStatus enum values; a snapshot timestamp
        is filled in server-side when updated_at is omitted. returns
        not_found when the task name is unknown."""
        tool = "set_task_status"
        started = time.monotonic()
        try:
            from datetime import datetime, timezone

            from cie.tasks import TaskStatus

            status_obj = TaskStatus(str(status))
            stamped = str(updated_at) or datetime.now(timezone.utc).isoformat()
            updated = self._task_repo.set_status(name, status_obj, stamped)
            if not updated:
                return self._err(
                    tool, "not_found", f"task '{name}' not found", started,
                    hint="call list_pending_tasks to see available tasks",
                )
        except ValueError:
            from cie.tasks import TaskStatus

            return self._err(
                tool, "validation", f"invalid status '{status}'", started,
                hint=f"valid statuses: {[s.value for s in TaskStatus]}",
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, {"name": name, "status": status_obj.value, "updated_at": stamped},
            started,
        )

    def link_artifact(
        self, task_name: str = "", kind: str = "", path: str = "",
        commit_sha: str = "",
    ) -> dict:
        """PRODUCED edge task -> artifact (T3.2): record that `task_name`
        produced the file at `path` (kind: source|test|schema|doc; the
        commit_sha pins the exact revision when known)."""
        tool = "link_artifact"
        started = time.monotonic()
        try:
            from cie.tasks import Artifact, ArtifactKind

            kind_obj = ArtifactKind(str(kind or ArtifactKind.SOURCE.value))
            artifact = Artifact(path=str(path), kind=kind_obj, commit_sha=str(commit_sha))
            linked = self._task_repo.add_artifact(task_name, artifact)
            if not linked:
                return self._err(
                    tool, "not_found", f"task '{task_name}' not found", started,
                    hint="call list_pending_tasks to see available tasks",
                )
        except ValueError:
            from cie.tasks import ArtifactKind

            return self._err(
                tool, "validation", f"invalid kind '{kind}'", started,
                hint=f"valid kinds: {[k.value for k in ArtifactKind]}",
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, {"task": task_name, "path": path, "kind": kind_obj.value},
            started,
        )

    def append_repair_events(
        self, task_name: str = "", events: Optional[list] = None
    ) -> dict:
        """Repair-trajectory write-back (T3.2): trajectory.jsonl rows ->
        HAS_EVENT edges. Each event: {round, failures_before,
        failures_after, mode, lint_ok?, timestamp?}."""
        tool = "append_repair_events"
        started = time.monotonic()
        try:
            from cie.tasks import RepairEvent

            parsed = [RepairEvent.model_validate(e) for e in (events or [])]
        except Exception as exc:  # noqa: BLE001
            return self._err(
                tool, "validation", str(exc), started,
                hint="events must match the RepairEvent shape: {round, "
                     "failures_before, failures_after, mode, lint_ok, timestamp}",
            )
        try:
            for event in parsed:
                if not self._task_repo.add_event(task_name, event):
                    return self._err(
                        tool, "not_found", f"task '{task_name}' not found", started,
                        hint="call list_pending_tasks to see available tasks",
                    )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, {"task": task_name, "events_appended": len(parsed)}, started,
        )

    def record_coverage(
        self, file_path: str = "", subtree: str = "",
        coverage_pct: float = 0.0, covered_lines: int = 0,
        uncovered_lines: Optional[list] = None, measured_at: str = "",
    ) -> dict:
        """Write one file's coverage measurement + derived per-function
        breakdown (QA workflow). Requires the file already indexed.
        Envelope: per-function breakdown with measured percentages; call
        before committing so QA tools can report real coverage."""
        tool = "record_coverage"
        started = time.monotonic()
        try:
            from datetime import datetime, timezone

            if not file_path:
                return self._err(
                    tool, "validation", "file_path is required", started,
                    hint="pass the same repo-relative path the coverage tool reported",
                )
            stamped = str(measured_at) or datetime.now(timezone.utc).isoformat()
            result = self._engine.record_coverage(
                file_path, subtree, float(coverage_pct), int(covered_lines),
                list(uncovered_lines or []), stamped,
            )
            if result is None:
                return self._err(
                    tool, "not_found", f"no indexed FILE node for '{file_path}'", started,
                    hint="index the file first (cie load/reindex) before "
                         "recording coverage",
                )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, dataclasses.asdict(result), started)

    def record_coverage_snapshot(
        self, aggregate_pct: float = 0.0, files_measured: int = 0,
        total_lines: int = 0, covered_lines: int = 0, subtree: str = "",
        measured_at: str = "",
    ) -> dict:
        """Append one aggregate coverage snapshot for trend history (QA
        workflow). Always a CREATE — never overwrites the prior snapshot;
        coverage_trend reads these back most-recent-first."""
        tool = "record_coverage_snapshot"
        started = time.monotonic()
        try:
            from datetime import datetime, timezone

            from cie.models import CoverageSnapshot

            snapshot = CoverageSnapshot(
                project=self._project,
                subtree=str(subtree),
                aggregate_pct=float(aggregate_pct),
                files_measured=int(files_measured),
                total_lines=int(total_lines),
                covered_lines=int(covered_lines),
                measured_at=str(measured_at)
                or datetime.now(timezone.utc).isoformat(),
            )
            self._engine.record_coverage_snapshot(snapshot)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(tool, dataclasses.asdict(snapshot), started)

    # -- PRD hierarchy — promoted from HTTP-only alias handlers (R14) ------

    def _require_hierarchy(self, tool: str, started: float):
        """The hierarchy store, or the honest unavailable envelope."""
        if self._hierarchy_repo is None:
            return None, self._err(
                tool, "unavailable",
                "no hierarchy store is configured for this backend", started,
                reason="HIERARCHY_STORE_NOT_CONFIGURED",
                hint="build the service with build_tool_service_embedded "
                     "(default) or a Neo4j-backed factory; pass --no-"
                     "hierarchy only when task/QA tracking is intentionally off",
            )
        return self._hierarchy_repo, None

    def push_hierarchy(self, tree: Optional[dict] = None) -> dict:
        """Pull an INS IDE planner tree (Module/Feature/Workflow/UseCase/
        UserStory — the HierarchyNode JSON shape) into the graph (§5.3).
        PRD-hierarchy write side; the tree is validated
        (ids unique on every root-to-leaf path) and MERGEed idempotently
        by (node id, project). UserStory nodes with
        metadata['task_names'] get REALIZED_BY links."""
        tool = "push_hierarchy"
        started = time.monotonic()
        repo, err = self._require_hierarchy(tool, started)
        if err is not None:
            return err
        try:
            from cie.tasks import HierarchyNode

            root = HierarchyNode.model_validate({k: v for k, v in (tree or {}).items()})
            written = repo.push_hierarchy(root, project=self._project)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool,
            {"nodes_written": written, "root_id": root.id,
             "root_name": root.name, "project": self._project},
            started,
        )

    def get_children(
        self, node_id: str = "", depth: int = 0, type_filter: str = "",
        limit: int = 200,
    ) -> dict:
        """THE 'all children' API for the PRD hierarchy (§5.3): BFS over
        HAS_CHILD ordered by depth then name. depth=0 unlimited; the
        type_filter restricts RESULTS (traversal passes through other
        types); when more exist, truncated=True. Envelope includes the
        root node + its computed children."""
        tool = "get_children"
        started = time.monotonic()
        repo, err = self._require_hierarchy(tool, started)
        if err is not None:
            return err
        try:
            subtree = repo.get_children(
                node_id, depth=int(depth), type_filter=str(type_filter),
                limit=int(limit),
            )
        except ValueError:
            return self._err(
                tool, "not_found", f"unknown hierarchy node '{node_id}'", started,
                hint="push the hierarchy first with push_hierarchy",
            )
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        hint = (
            "children capped at the limit; narrow with depth or type_filter"
            if subtree.truncated
            else None
        )
        return self._ok(
            tool,
            {
                "root": subtree.root.model_dump(mode="json"),
                "children": [c.model_dump(mode="json") for c in subtree.children],
            },
            started, truncated=subtree.truncated, hint=hint,
        )

    def get_lineage(self, node_id: str = "") -> dict:
        """Ancestor path of one hierarchy node, root-first (§5.3) — "which
        module does this user story belong to", in one hop query."""
        tool = "get_lineage"
        started = time.monotonic()
        repo, err = self._require_hierarchy(tool, started)
        if err is not None:
            return err
        try:
            lineage = repo.get_lineage(node_id)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [v.model_dump(mode="json") for v in lineage]
        return self._ok(
            tool, results, started,
            hint=None if results else (
                f"unknown hierarchy node '{node_id}'; push the hierarchy "
                "first with push_hierarchy"
            ),
        )
