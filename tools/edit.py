"""File mutation tools (``write_file``, ``edit_file``) — jailed under root.

Mirrors Claude Code's own Write/Edit tool semantics: ``write_file`` creates
or overwrites wholesale; ``edit_file`` does an exact string replace and
requires the match to be unique unless ``replace_all`` is set.
"""

from __future__ import annotations

from pathlib import Path

from cie.tools.view import _jail


def write_file(root: Path, path: str, content: str) -> dict:
    """Create or overwrite a file with ``content``, jailed under ``root``.

    Args:
        root: Project root; ``path`` is jailed under it.
        path: File path, relative to ``root`` (absolute paths accepted but
            must still resolve inside ``root``).
        content: Full file contents to write.

    Returns:
        ``{path, bytes_written, created}`` — ``created`` is True when the
        file did not exist before this call.

    Raises:
        ValueError: If ``path`` escapes ``root``, or resolves to a
            directory.
    """
    resolved = _jail(root, path)
    if resolved.is_dir():
        raise ValueError(f"{path!r} is a directory, not a file")
    existed = resolved.is_file()
    resolved.parent.mkdir(parents=True, exist_ok=True)
    resolved.write_text(content)
    return {
        "path": path,
        "bytes_written": len(content.encode()),
        "created": not existed,
    }


def edit_file(
    root: Path,
    path: str,
    old_string: str,
    new_string: str,
    replace_all: bool = False,
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

    Returns:
        ``{path, replacements}``.

    Raises:
        ValueError: If ``path`` escapes ``root``; if ``old_string`` is not
            found; if ``old_string`` and ``new_string`` are identical; or
            if ``old_string`` matches more than once and ``replace_all``
            is not set.
        FileNotFoundError: If the file does not exist.
    """
    resolved = _jail(root, path)
    if not resolved.is_file():
        raise FileNotFoundError(f"no such file under project root: {path}")
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
