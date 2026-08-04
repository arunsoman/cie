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

import dataclasses
import inspect
import time
from pathlib import Path
from typing import Any, Optional

from cie import extract
from cie.models import Node

from cie.tools import blame as _blame
from cie.tools import edit as _edit
from cie.tools import runner as _runner
from cie.tools import view as _view

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
        """Windowed file view joined with the graph skeleton (T1.1)."""
        tool = "view_file"
        started = time.monotonic()
        try:
            skeleton = self._engine.get_file_skeleton(path)
            result = _view.view_file(self._root, path, start, end, skeleton)
        except Exception as exc:  # noqa: BLE001 - converted to envelope
            return self._guard(tool, started, exc)
        window = result["window"]
        truncated = window["end"] < result["total_lines"]
        return self._ok(tool, [result], started, truncated=truncated,
                        hint=result["hint"])

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
        try:
            matches = self._engine.search_symbols(name, kind, file_glob, limit)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
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
            for m in matches
        ]
        if not results:
            return self._ok(
                tool, results, started,
                hint=f"no symbol named '{name}'; try a substring or drop the "
                     "kind filter",
            )
        truncated = len(results) >= limit
        hint = (
            f"results capped at {limit}; refine with kind or file_glob"
            if truncated
            else None
        )
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

    def callers(self, symbol: str, limit: int = 30) -> dict:
        """Blast radius: who calls ``symbol`` (T1.3)."""
        tool = "callers"
        started = time.monotonic()
        try:
            records = self._engine.get_callers(symbol, limit)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = self._edge_results(records, "caller")
        if not results:
            return self._ok(tool, results, started, hint=HINT_EMPTY_CALLERS)
        truncated = len(results) >= limit
        return self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {limit}" if truncated else None,
        )

    def callees(self, symbol: str, limit: int = 30) -> dict:
        """Reverse localization: what ``symbol`` calls (T1.3)."""
        tool = "callees"
        started = time.monotonic()
        try:
            records = self._engine.get_callees(symbol, limit)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = self._edge_results(records, "callee")
        if not results:
            return self._ok(tool, results, started, hint=HINT_EMPTY_CALLERS)
        truncated = len(results) >= limit
        return self._ok(
            tool, results, started, truncated=truncated,
            hint=f"results capped at {limit}" if truncated else None,
        )

    def file_skeleton(self, path: str) -> dict:
        """Signatures + line ranges for every symbol in a file, no bodies (T2.1)."""
        tool = "file_skeleton"
        started = time.monotonic()
        try:
            nodes = self._engine.get_file_skeleton(path)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        symbols = [
            {
                "name": n.label,
                "kind": n.kind,
                "signature": n.signature,
                "lines": [n.line_start, n.line_end],
                "docstring_first_line": n.docstring,
            }
            for n in nodes
            if n.kind != "file"  # file hub is not a symbol
        ]
        result = {"path": path, "symbols": symbols}
        hint = (
            None
            if symbols
            else f"no symbols indexed for '{path}'; load/reindex the file first"
        )
        return self._ok(tool, [result], started, hint=hint)

    def failing_context(
        self, test_identifier: str, depth: int = 3, limit: int = 30
    ) -> dict:
        """Symbols reachable from a failing test, ranked by distance (T2.3)."""
        tool = "failing_context"
        started = time.monotonic()
        try:
            hits = self._engine.failing_context(test_identifier, depth, limit)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "distance": h.distance,
                "file": h.node.source_file,
                "symbol": h.node.label,
                "confidence": _confidence_value(h.confidence),
            }
            for h in hits
        ]
        if not results:
            return self._ok(tool, results, started,
                            hint=HINT_EMPTY_FAILING_CONTEXT)
        return self._ok(tool, results, started,
                        hint="distance-1 symbols are the prime suspects")

    def path_between(self, source: str, target: str, max_hops: int = 8) -> dict:
        """Shortest call/contains path as ordered per-hop chain (T2.2)."""
        tool = "path_between"
        started = time.monotonic()
        try:
            path = self._engine.shortest_path(source, target, max_hops)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        if path is None or not path.nodes:
            return self._ok(
                tool, [], started,
                hint=f"no path between '{source}' and '{target}' within "
                     f"{max_hops} hops; check the symbol names or raise "
                     "max_hops",
            )
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
        try:
            hits = self._engine.affected_by(file_path, max_depth, direction, max_results)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        results = [
            {
                "distance": h.distance,
                "file": h.node.source_file,
                "symbol": h.node.label,
                "confidence": _confidence_value(h.confidence),
            }
            for h in hits
        ]
        if not results:
            return self._ok(
                tool, results, started,
                hint=f"no nodes found for '{file_path}' within {max_depth} hops "
                     f"({direction}); check the path or raise max_depth",
            )
        return self._ok(tool, results, started,
                        hint="distance-1 hits are the most directly affected")

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
        hint = None
        if extract.supported_suffix(resolved) is not None:
            try:
                self._engine._repo.reindex_file(str(resolved), extract.Extraction())  # noqa: SLF001
            except Exception as exc:  # noqa: BLE001
                hint = f"file deleted but graph cleanup failed: {exc}"
        return self._ok(tool, [result], started, hint=hint)

    def _sync_graph_after_write(self, path: str, result: dict) -> tuple[dict, Optional[str]]:
        """Best-effort incremental reindex right after a write/edit landed
        on disk. Parses the just-written file and re-runs pass-2 call-edge
        resolution for it (see ``Repository.reindex_file``) so the graph
        never has a window where the caller has to remember a second step.

        Non-source files (no supported suffix) are left alone — nothing to
        index — and any indexing failure (bad parse, Neo4j unreachable)
        degrades to a hint rather than failing the write, which already
        landed on disk successfully by this point."""
        resolved = _view._jail(self._root, path)
        if extract.supported_suffix(resolved) is None:
            return result, "not an indexable source file; graph unaffected"
        try:
            extraction = extract.extract_file(resolved)
            written = self._engine._repo.reindex_file(str(resolved), extraction)  # noqa: SLF001
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
            extraction = extract.extract_file(resolved)
            written = self._engine._repo.reindex_file(str(resolved), extraction)
        except Exception as exc:  # noqa: BLE001
            return self._guard(tool, started, exc)
        return self._ok(
            tool, [{"path": str(resolved), "nodes_written": written}], started,
            hint="graph is fresh for this file; callers of unchanged files "
                 "were re-resolved in the same call",
        )

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
