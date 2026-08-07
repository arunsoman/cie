"""Windowed, line-numbered file views (the ``view_file`` tool).

One consolidated reading action: returns a bounded window of a source file
with 1-based line numbers (patch blocks and edit commands reference them)
plus a symbol index joining the window against the knowledge graph's
skeleton nodes for the file.
"""

from __future__ import annotations

import threading
from collections import OrderedDict
from pathlib import Path
from typing import Sequence

from cie.models import Node

#: Default window size in lines (integration spec §0.2: ~100 lines per call).
DEFAULT_WINDOW = 100

#: path -> (mtime_ns, size, lines). Keyed by (mtime, size) rather than a
#: plain path->lines cache with explicit invalidation calls from
#: write_file/edit_file/delete_file: a forge run's agent loop calls
#: view_file on the SAME file many times per generation session (skeleton
#: first, then windowed reads), and re-reading + re-splitting from disk
#: every time was pure waste when nothing changed. mtime/size staleness
#: checking also covers writers this module never hears from at all (a
#: human editing the checkout, another agent, a git merge landing mid-run)
#: — the same class of "don't trust a cache that can't see external
#: writes" concern CieBackend.start_watch's subprocess watcher exists for.
_LINE_CACHE: "OrderedDict[str, tuple[tuple[int, int], tuple[str, ...]]]" = OrderedDict()
_CACHE_LOCK = threading.Lock()
_CACHE_MAX_ENTRIES = 512


def _read_lines_cached(resolved: Path) -> list[str]:
    key = str(resolved)
    st = resolved.stat()
    stamp = (st.st_mtime_ns, st.st_size)
    with _CACHE_LOCK:
        cached = _LINE_CACHE.get(key)
        if cached is not None and cached[0] == stamp:
            _LINE_CACHE.move_to_end(key)
            return list(cached[1])
    lines = resolved.read_text(errors="replace").splitlines()
    with _CACHE_LOCK:
        _LINE_CACHE[key] = (stamp, tuple(lines))
        _LINE_CACHE.move_to_end(key)
        while len(_LINE_CACHE) > _CACHE_MAX_ENTRIES:
            _LINE_CACHE.popitem(last=False)
    return lines


def _jail(root: Path, path: str) -> Path:
    """Resolve ``path`` under ``root`` and refuse escapes.

    Args:
        root: Project root the view is jailed under.
        path: User-supplied (possibly relative) path.

    Returns:
        The resolved absolute path.

    Raises:
        ValueError: If the resolved path escapes ``root``.
    """
    root_resolved = Path(root).resolve()
    resolved = (root_resolved / path).resolve() if not Path(path).is_absolute() else Path(path).resolve()
    if resolved != root_resolved and root_resolved not in resolved.parents:
        raise ValueError(
            f"path {path!r} escapes root {root_resolved}; "
            "all file tools are jailed inside the project directory"
        )
    return resolved


def _symbols_in_window(
    skeleton: Sequence[Node], start: int, end: int
) -> list[dict]:
    """Skeleton nodes whose [line_start, line_end] intersects the window."""
    out: list[dict] = []
    for node in skeleton:
        if node.line_start <= 0 or node.line_end <= 0:
            continue  # node carries no line info (e.g. a file hub)
        if node.line_end < start or node.line_start > end:
            continue
        entry = {"name": node.label, "kind": node.kind}
        if node.signature:
            entry["signature"] = node.signature
        entry["lines"] = [node.line_start, node.line_end]
        out.append(entry)
    return out


def view_file(
    root: Path,
    path: str,
    start: int = 1,
    end: int = DEFAULT_WINDOW,
    skeleton: Sequence[Node] = (),
) -> dict:
    """Return a windowed, line-numbered view of one file.

    Args:
        root: Project root; ``path`` is jailed under it.
        path: File path, relative to ``root`` (absolute paths are accepted
            but must still resolve inside ``root``).
        start: First line of the window, 1-based, clamped to the file.
        end: Last line of the window, inclusive, clamped to the file.
        skeleton: Symbol nodes for this file (from
            ``QueryEngine.get_file_skeleton``); nodes whose line range
            intersects the window appear in ``symbol_index``.

    Returns:
        A dict ``{path, total_lines, window: {start, end}, content,
        symbol_index, hint}``. ``content`` lines are formatted
        ``f"{n:>5}\\t{line}"``. ``hint`` tells the agent how to page forward
        and is ``None`` once the window reaches end-of-file.

    Raises:
        ValueError: If ``path`` escapes ``root``.
        FileNotFoundError: If the file does not exist (the surface layer
            converts this into an error envelope with a hint).
    """
    resolved = _jail(root, path)
    if not resolved.is_file():
        raise FileNotFoundError(f"no such file under project root: {path}")

    lines = _read_lines_cached(resolved)
    total = len(lines)

    start_clamped = max(1, start)
    end_clamped = min(end, total)
    if start_clamped > end_clamped:
        # Window starts past EOF (or the file is empty): empty content and
        # no paging hint, since there is nothing further to fetch.
        return {
            "path": path,
            "total_lines": total,
            "window": {"start": start_clamped, "end": end_clamped},
            "content": "",
            "symbol_index": [],
            "hint": None,
        }

    content = "\n".join(
        f"{n:>5}\t{lines[n - 1]}" for n in range(start_clamped, end_clamped + 1)
    )
    hint = None
    if end_clamped < total:
        hint = (
            f"window {start_clamped}-{end_clamped} of {total}; "
            f"call again with start={end_clamped + 1} for more"
        )

    return {
        "path": path,
        "total_lines": total,
        "window": {"start": start_clamped, "end": end_clamped},
        "content": content,
        "symbol_index": _symbols_in_window(skeleton, start_clamped, end_clamped),
        "hint": hint,
    }
