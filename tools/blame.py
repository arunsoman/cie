"""Git blame history (the ``blame_history`` tool).

Pure git, no database: ``git log --follow`` for one path, parsed into
machine-readable entries. The join against the task graph (PRODUCED
artifacts) happens one layer up, in :class:`cie.tools.ToolService`,
via ``TaskRepository.artifacts_for_path``.
"""

from __future__ import annotations

import subprocess
import threading
import time
from pathlib import Path

#: Field separator inside the git --format string (unit separator, safe in
#: commit subjects).
_SEP = "\x1f"

_FORMAT = f"%H%x1f%ad%x1f%s"

#: (repo_root, path, limit) -> (cached_at, entries). Git history for an
#: already-committed path is immutable except when a NEW commit lands
#: touching it — forking `git log` on every call (this tool is called
#: repeatedly per file across a forge session) redoes work that's usually
#: still correct a few seconds later. A short TTL, not mtime-based
#: invalidation like view.py's file cache: there's no single cheap
#: signal (no directory mtime covers "a new commit touched this path")
#: to check without running git again, which would defeat the point.
_BLAME_CACHE_TTL_S = 60.0
_BLAME_CACHE: dict[tuple[str, str, int], tuple[float, list[dict]]] = {}
_BLAME_CACHE_LOCK = threading.Lock()


def blame_history(repo_root: Path, path: str, limit: int = 20) -> list[dict]:
    """Return recent commit history for ``path`` under ``repo_root``.

    Runs ``git log --follow --format=%H%x1f%ad%x1f%s --date=iso-strict`` so
    renames are tracked across history. Cached for `_BLAME_CACHE_TTL_S`
    seconds per (repo_root, path, limit); always returns a fresh copy of
    the cached entries (never the same dict objects across calls) since
    `ToolService.blame_history` mutates each entry in place (stamping a
    `task` key) after calling this — sharing dict objects across calls
    would leak one caller's mutation into the next.

    Args:
        repo_root: Git working-tree root; resolved before use. ``path`` is
            interpreted relative to it.
        path: File path whose history to list.
        limit: Maximum number of entries (bounded-response rule; newest
            first).

    Returns:
        A list of ``{commit_sha, message, timestamp}`` dicts, newest first.
        Returns ``[]`` — never raises — when ``repo_root`` is not a git
        repository, git fails, or the path has no recorded history. This is
        deliberate ACI design: agents misread subprocess errors as tool
        breakage, so the empty result plus a surface-level hint ("no git
        history for '<path>'; is it committed?") is the correct contract.
    """
    root = Path(repo_root).resolve()
    key = (str(root), path, int(limit))
    now = time.monotonic()
    with _BLAME_CACHE_LOCK:
        cached = _BLAME_CACHE.get(key)
        if cached is not None and now - cached[0] < _BLAME_CACHE_TTL_S:
            return [dict(e) for e in cached[1]]

    entries = _run_git_log(root, path, limit)

    with _BLAME_CACHE_LOCK:
        _BLAME_CACHE[key] = (now, entries)
    return [dict(e) for e in entries]


def _run_git_log(root: Path, path: str, limit: int) -> list[dict]:
    if not root.is_dir():
        return []

    try:
        proc = subprocess.run(
            [
                "git",
                "log",
                "--follow",
                f"--format={_FORMAT}",
                "--date=iso-strict",
                f"-n{int(limit)}",
                "--",
                path,
            ],
            cwd=str(root),
            capture_output=True,
            text=True,
            errors="replace",
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired):
        return []

    if proc.returncode != 0:
        # Not a git repo (128), bad revision, etc. — all degrade to [].
        return []

    entries: list[dict] = []
    for line in proc.stdout.splitlines():
        if not line.strip():
            continue
        parts = line.split(_SEP)
        if len(parts) != 3:
            continue  # defensive: a subject containing \x1f would mis-split
        sha, timestamp, message = parts
        entries.append(
            {"commit_sha": sha, "message": message, "timestamp": timestamp}
        )
    return entries
