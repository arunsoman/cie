"""File mutation tools (``write_file``, ``edit_file``) — jailed under root.

Mirrors Claude Code's own Write/Edit tool semantics: ``write_file`` creates
or overwrites wholesale; ``edit_file`` does an exact string replace and
requires the match to be unique unless ``replace_all`` is set.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional

from cie.tools.view import DEFAULT_MAX_FILE_SIZE_BYTES, _jail


def write_file(
    root: Path,
    path: str,
    content: str,
    max_file_size_bytes: int = DEFAULT_MAX_FILE_SIZE_BYTES,
) -> dict:
    """Create or overwrite a file with ``content``, jailed under ``root``.

    Args:
        root: Project root; ``path`` is jailed under it.
        path: File path, relative to ``root`` (absolute paths accepted but
            must still resolve inside ``root``).
        content: Full file contents to write.
        max_file_size_bytes: Refuse to write content bigger than this —
            see ``cie.tools.view.DEFAULT_MAX_FILE_SIZE_BYTES``.

    Returns:
        ``{path, bytes_written, created}`` — ``created`` is True when the
        file did not exist before this call.

    Raises:
        ValueError: If ``path`` escapes ``root``, resolves to a directory,
            or ``content`` is over ``max_file_size_bytes``.
    """
    resolved = _jail(root, path)
    if resolved.is_dir():
        raise ValueError(f"{path!r} is a directory, not a file")
    size = len(content.encode())
    if size > max_file_size_bytes:
        raise ValueError(
            f"{path!r} content is {size} bytes, over the "
            f"{max_file_size_bytes}-byte write ceiling"
        )
    existed = resolved.is_file()
    resolved.parent.mkdir(parents=True, exist_ok=True)
    resolved.write_text(content)
    return {
        "path": path,
        "bytes_written": size,
        "created": not existed,
    }


def write_files_atomic(
    root: Path,
    files: dict[str, str],
    max_file_size_bytes: int = DEFAULT_MAX_FILE_SIZE_BYTES,
) -> dict[str, dict]:
    """Write every path in ``files`` (path -> content), all-or-nothing.

    Every target path is jail-resolved and its prior state snapshotted
    (original content, or "did not exist") BEFORE any write happens. If
    any file in the batch fails to write, every file already written in
    this call is restored to its prior state (content rewritten back, or
    deleted if it did not exist before) and the triggering exception
    re-raises — a caller that gets an exception from this function can
    assume the filesystem is exactly as it was before the call.

    Args:
        root: Project root; every path in ``files`` is jailed under it.
        files: Mapping of relative path -> full new content.
        max_file_size_bytes: Per-file ceiling, checked for EVERY file
            before any write happens, so a batch never partially lands on
            a late oversized file.

    Returns:
        ``{path: {path, bytes_written, created}}`` — one entry per file,
        same per-file shape ``write_file`` returns.

    Raises:
        ValueError: A path escapes ``root``, targets a directory, or its
            content is over ``max_file_size_bytes`` — raised before any
            write in the batch happens.
        OSError: A real disk error mid-batch — triggers rollback of
            whatever in this batch had already been written, then
            re-raises.
    """
    resolved_by_path: dict[str, Path] = {}
    for path in files:
        resolved = _jail(root, path)
        if resolved.is_dir():
            raise ValueError(f"{path!r} is a directory, not a file")
        resolved_by_path[path] = resolved

    for path, content in files.items():
        size = len(content.encode())
        if size > max_file_size_bytes:
            raise ValueError(
                f"{path!r} content is {size} bytes, over the "
                f"{max_file_size_bytes}-byte write ceiling"
            )

    # Snapshot BEFORE any write in this batch touches disk.
    prior_state: dict[str, Optional[str]] = {
        path: (resolved.read_text() if resolved.is_file() else None)
        for path, resolved in resolved_by_path.items()
    }

    written: list[str] = []
    try:
        results: dict[str, dict] = {}
        for path, content in files.items():
            resolved = resolved_by_path[path]
            existed = prior_state[path] is not None
            resolved.parent.mkdir(parents=True, exist_ok=True)
            resolved.write_text(content)
            written.append(path)
            results[path] = {
                "path": path,
                "bytes_written": len(content.encode()),
                "created": not existed,
            }
        return results
    except Exception:
        for path in written:
            resolved = resolved_by_path[path]
            prior = prior_state[path]
            if prior is None:
                resolved.unlink(missing_ok=True)
            else:
                resolved.write_text(prior)
        raise


def edit_file(
    root: Path,
    path: str,
    old_string: str,
    new_string: str,
    replace_all: bool = False,
    max_file_size_bytes: int = DEFAULT_MAX_FILE_SIZE_BYTES,
) -> dict:
    """Exact string replace in a file under ``root``.

    Args:
        root: Project root; ``path`` is jailed under it.
        path: File path, relative to ``root``.
        old_string: Exact text to find. Must match exactly once unless
            ``replace_all`` is set.
        new_string: Replacement text.
        replace_all: Replace every occurrence instead of requiring a
            unique match.
        max_file_size_bytes: Refuse to view the source file, or write the
            result, if either is bigger than this — see
            ``cie.tools.view.DEFAULT_MAX_FILE_SIZE_BYTES``.

    Returns:
        ``{path, replacements}``.

    Raises:
        ValueError: If ``path`` escapes ``root``; if the file (before or
            after the edit) is over ``max_file_size_bytes``; if
            ``old_string`` is not found; if ``old_string`` and
            ``new_string`` are identical; or if ``old_string`` matches
            more than once and ``replace_all`` is not set.
        FileNotFoundError: If the file does not exist.
    """
    resolved = _jail(root, path)
    if not resolved.is_file():
        raise FileNotFoundError(f"no such file under project root: {path}")
    size = resolved.stat().st_size
    if size > max_file_size_bytes:
        raise ValueError(
            f"{path!r} is {size} bytes, over the {max_file_size_bytes}-byte "
            "edit ceiling"
        )
    text = resolved.read_text()
    count = text.count(old_string)
    if count == 0:
        raise ValueError(f"old_string not found in {path!r}")
    if old_string == new_string:
        raise ValueError(
            "old_string and new_string are identical; nothing to change"
        )
    if not replace_all and count > 1:
        raise ValueError(
            f"old_string matches {count} locations in {path!r}; add more "
            "surrounding context to make it unique, or pass replace_all=True"
        )
    new_text = (
        text.replace(old_string, new_string)
        if replace_all
        else text.replace(old_string, new_string, 1)
    )
    new_size = len(new_text.encode())
    if new_size > max_file_size_bytes:
        raise ValueError(
            f"edit result for {path!r} would be {new_size} bytes, over the "
            f"{max_file_size_bytes}-byte edit ceiling; file left unchanged"
        )
    resolved.write_text(new_text)
    return {"path": path, "replacements": count if replace_all else 1}


def delete_file(root: Path, path: str) -> dict:
    """Delete a file, jailed under ``root``.

    Args:
        root: Project root; ``path`` is jailed under it.
        path: File path, relative to ``root``.

    Returns:
        ``{path, deleted}``.

    Raises:
        ValueError: If ``path`` escapes ``root``, or resolves to a
            directory.
        FileNotFoundError: If the file does not exist.
    """
    resolved = _jail(root, path)
    if resolved.is_dir():
        raise ValueError(f"{path!r} is a directory, not a file")
    if not resolved.is_file():
        raise FileNotFoundError(f"no such file under project root: {path}")
    resolved.unlink()
    return {"path": path, "deleted": True}
