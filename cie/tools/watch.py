"""Shared debounced filesystem-watch reindex handler.

One implementation, two entry points that must not drift apart: ``cie
watch`` (cli.py, run as a standalone CLI subcommand — its own process,
started by hand or by whatever operates a separate cie deployment, not by
anything in this codebase today) and ``ToolService.start_watch()``
(in-process ``watchdog.observers.Observer`` thread — used by both the HTTP
tool surface and, since 2026-08-07, forge's own in-process
``CieBackend.start_watch()``, which used to spawn a SEPARATE ``cie watch``
subprocess itself and now just delegates here instead; see
``CieBackend.start_watch``'s docstring for the trade-off that collapse
made). Before this module existed, cli.py's ``_build_reindex_handler`` and
a second, independently maintained copy in ``ToolService.start_watch``
would have drifted the same way the SEARCH/REPLACE parser did (see
``forge/patch.py``'s docstring for that history) — one handler, imported
by both, closes that off structurally.
"""

from __future__ import annotations

import threading
from pathlib import Path
from typing import Any, Callable, Optional

#: Called after each debounced reindex attempt: (path, nodes_written or
#: None, exception or None). Lets each caller report the outcome its own
#: way (cli.py prints to the console; ToolService has no console to print
#: to) without this module taking a dependency on either.
OnReindexed = Callable[[str, Optional[int], Optional[Exception]], None]


def build_reindex_handler(repo: Any, project: str, debounce: float,
                          on_reindexed: Optional[OnReindexed] = None):
    """A ``watchdog.events.FileSystemEventHandler`` that debounces rapid-fire
    filesystem events per path and calls ``repo.reindex_file`` once they
    settle.

    Imported lazily by callers (not at module import time) so importing
    ``cie.cli`` or ``cie.tools`` never requires ``watchdog`` to be installed
    unless a watch is actually started.
    """
    from watchdog.events import FileSystemEventHandler

    class _ReindexHandler(FileSystemEventHandler):
        def __init__(self) -> None:
            self._timers: dict[str, threading.Timer] = {}
            self._lock = threading.Lock()

        def _schedule(self, path: str) -> None:
            from cie.extract import supported_suffix

            if supported_suffix(Path(path)) is None:
                return
            # Same ignored-directory convention ToolService.reindex()'s bulk
            # walk already applies (cie/tools/__init__.py) — without it, a
            # watched project root (recursive=True, no path filter of its
            # own) schedules a reindex for every file `npm install` writes
            # under node_modules, polluting the graph with third-party
            # internals. Confirmed live: a minified axios bundle's mangled
            # `it` symbol got indexed this way and then won every project's
            # test files' `it(...)` call-site resolution via callgraph.py's
            # rule (d) "globally unique same-named symbol" fallback.
            if any(
                part.startswith(".") or part in ("node_modules", "__pycache__", ".venv")
                for part in Path(path).parts
            ):
                return
            with self._lock:
                existing = self._timers.get(path)
                if existing is not None:
                    existing.cancel()
                timer = threading.Timer(debounce, self._reindex, args=(path,))
                timer.daemon = True
                self._timers[path] = timer
                timer.start()

        def _reindex(self, path: str) -> None:
            from cie.extract import Extraction, extract_file

            try:
                p = Path(path)
                extraction = extract_file(p) if p.is_file() else Extraction()
                count = repo.reindex_file(path, extraction, project=project)
                if on_reindexed is not None:
                    on_reindexed(path, count, None)
            except Exception as exc:  # noqa: BLE001 - keep the watcher alive
                if on_reindexed is not None:
                    on_reindexed(path, None, exc)

        def on_created(self, event) -> None:
            if not event.is_directory:
                self._schedule(event.src_path)

        def on_modified(self, event) -> None:
            if not event.is_directory:
                self._schedule(event.src_path)

        def on_deleted(self, event) -> None:
            if not event.is_directory:
                self._schedule(event.src_path)

        def on_moved(self, event) -> None:
            if not event.is_directory:
                self._schedule(event.src_path)
                self._schedule(event.dest_path)

    return _ReindexHandler()
