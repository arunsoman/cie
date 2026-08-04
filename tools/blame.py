"""Git blame history (the ``blame_history`` tool).

Pure git, no database: ``git log --follow`` for one path, parsed into
machine-readable entries. The join against the task graph (PRODUCED
artifacts) happens one layer up, in :class:`cie.tools.ToolService`,
via ``TaskRepository.artifacts_for_path``.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

#: Field separator inside the git --format string (unit separator, safe in
#: commit subjects).
_SEP = "\x1f"

_FORMAT = f"%H%x1f%ad%x1f%s"


def blame_history(repo_root: Path, path: str, limit: int = 20) -> list[dict]:
    """Return recent commit history for ``path`` under ``repo_root``.

    Runs ``git log --follow --format=%H%x1f%ad%x1f%s --date=iso-strict`` so
    renames are tracked across history.

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
