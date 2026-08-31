"""Per-project file-path index — the in-process source behind ``ls`` /
``dir`` / ``file_hierarchy`` / ``file_names_like`` / ``path_prefix``.

Structure decision — why a sorted array, not a trie: this index exists
to answer "what's here?" and "where is anything shaped like X?" for an
LLM navigating a repo, as cheaply as possible in memory. A segment trie
buys O(prefix) queries at the cost of one dict per DIRECTORY (≈200
bytes minimum each in CPython) on top of the path strings themselves —
for a 10k-file repo with 1k directories that is megabytes of pure
structure. A SORTED LIST of path strings costs exactly one string per
file, and every query this surface needs reduces to binary search over
that one array, because all paths sharing a prefix are contiguous in
sorted order:

    ls / children   one bisect + a contiguous walk of the parent range
    file_hierarchy  the same walk, recursed depth-first under a budget
    path_prefix     bisect + contiguous walk (the "fast key searcher")
    file_names_like fnmatch over the array (a C loop) — O(n) per query
    files_within    two bisects (range ["src/", "src0") — '/' sorts
                    below every character a segment can start with)

The sync contract mirrors cie.ast_store exactly: reads serve only the
in-process index (one lazy ``os.walk`` per root per process, pruning
``cie.extract.EXCLUDED_DIRS`` so virtualenvs/node_modules/VCS internals/
build outputs — and cie's own .cie graph dir — never enter it), every
cie write path updates it in the SAME call as the filesystem write
(write_file/write_files_atomic/edit_file/apply_patch via
``_sync_graph_after_write``, delete_file removes, reindex_file/
sync_ast_delta refresh one path, reindex() rebuilds), and external
changes stay invisible until an explicit refresh — the price of reads
that never stat or walk the tree, and the guarantee that no tool can
ever see a half-applied write.

Honest limits, documented not hidden: the index tracks FILES, so an
empty directory (or one whose files were all excluded) is invisible;
file sizes/mtimes are not in the index (they would cost a stat per
file — get_meta/view_file remain the tools for content, ls stays the
tool for shape).
"""
from __future__ import annotations

import fnmatch
import os
import threading
from bisect import bisect_left
from pathlib import Path
from typing import Optional

from cie.ast_store import file_type_for
from cie.extract import EXCLUDED_DIRS

#: caps that keep every envelope bounded (LIST_CAP-style honest
#: truncation: real totals + truncated flag + a paging hint, never a
#: silent cut).
LS_CAP = 200
TREE_ENTRY_CAP = 300
MATCH_CAP = 50


class FileIndexError(Exception):
    """A navigation tool could not serve its request — carries the SPEC §0
    error `kind` and the actionable hint, mirroring
    ``cie.ast_store.AstLookupError``."""

    def __init__(self, kind: str, message: str, hint: Optional[str] = None):
        super().__init__(message)
        self.kind = kind
        self.hint = hint


def _upper_bound(prefix: str) -> str:
    """Exclusive upper bound for the sorted range of everything under
    `prefix` ('src/' -> 'src0'): '/' sorts below every character a
    segment can start with, so [prefix, prefix[:-1]+chr(next)) is exactly
    the subtree. Two bisects -> subtree size, no walking."""
    return prefix[:-1] + chr(ord(prefix[-1]) + 1)


class _ProjectFileIndex:
    """One root's index: a sorted list of relative posix path strings."""

    def __init__(self, root: Path):
        self.root = Path(root).resolve()
        self._paths: list[str] = []
        self._lock = threading.RLock()
        self._built = False

    # -- build / mutate ---------------------------------------------------

    def _walk(self) -> list[str]:
        out: list[str] = []
        for dirpath, dirnames, filenames in os.walk(self.root):
            # prune in place: excluded dirs are never entered at all, so a
            # .venv with 40k files costs nothing to skip
            dirnames[:] = [d for d in dirnames if d not in EXCLUDED_DIRS]
            rel = os.path.relpath(dirpath, self.root)
            prefix = "" if rel == "." else rel.replace(os.sep, "/") + "/"
            out.extend(prefix + name for name in filenames)
        out.sort()
        return out

    def ensure_built(self) -> None:
        if self._built:
            return
        with self._lock:
            if not self._built:
                self._paths = self._walk()
                self._built = True

    def rebuild(self) -> int:
        """Explicit refresh (reindex): re-walk, replace, return file count."""
        with self._lock:
            self._paths = self._walk()
            self._built = True
            return len(self._paths)

    def add(self, rel: str) -> None:
        with self._lock:
            lo = bisect_left(self._paths, rel)
            if lo < len(self._paths) and self._paths[lo] == rel:
                return
            self._paths.insert(lo, rel)

    def remove(self, rel: str) -> None:
        with self._lock:
            lo = bisect_left(self._paths, rel)
            if lo < len(self._paths) and self._paths[lo] == rel:
                self._paths.pop(lo)

    def refresh(self, rel: str, exists_on_disk: bool) -> None:
        (self.add if exists_on_disk else self.remove)(rel)

    # -- queries (all read a snapshot; never touch the filesystem) --------

    def snapshot(self) -> list[str]:
        with self._lock:
            return list(self._paths)

    def is_file(self, rel: str) -> bool:
        with self._lock:
            lo = bisect_left(self._paths, rel)
            return lo < len(self._paths) and self._paths[lo] == rel

    def count_under(self, prefix: str, paths: list[str]) -> int:
        """Files under the directory `prefix` ('' = all) in two bisects."""
        if not prefix:
            return len(paths)
        lo = bisect_left(paths, prefix)
        hi = bisect_left(paths, _upper_bound(prefix))
        return hi - lo

    def iter_children(self, prefix: str, paths: list[str]):
        """Immediate children of the directory `prefix` in sorted order —
        one bisect + one contiguous walk. Yields ``(name, is_dir)``; each
        directory appears once. Empty dirs are invisible (files define
        shape — see the module docstring)."""
        lo = bisect_left(paths, prefix)
        seen_dirs: set[str] = set()
        for path in paths[lo:]:
            if not path.startswith(prefix):
                return
            rest = path[len(prefix):]
            if not rest:
                continue
            name, sep, _ = rest.partition("/")
            if sep:
                if name not in seen_dirs:
                    seen_dirs.add(name)
                    yield name, True
            else:
                yield name, False

    def dirs_under(self, prefix: str, paths: list[str]) -> list[str]:
        """Every intermediate directory under `prefix` (derived, not
        stored — another thing the array gets for free that a trie would
        have to keep as nodes)."""
        out: set[str] = set()
        lo = bisect_left(paths, prefix)
        for path in paths[lo:]:
            if not path.startswith(prefix):
                break
            rest = path[len(prefix):]
            while "/" in rest:
                rest = rest.rsplit("/", 1)[0]
                out.add(prefix + rest)
        return sorted(out)


_INDEXES: dict[str, _ProjectFileIndex] = {}
_INDEXES_LOCK = threading.Lock()


def for_root(root: Path) -> _ProjectFileIndex:
    key = str(Path(root).resolve())
    with _INDEXES_LOCK:
        index = _INDEXES.get(key)
        if index is None:
            index = _ProjectFileIndex(Path(key))
            _INDEXES[key] = index
        return index


def rel_under(root: Path, resolved: Path) -> str:
    """Jailed-resolved path -> index key (relative posix). Outside-root
    paths (which _jail already refuses upstream) return '' and are
    ignored by every hook. The root itself normalizes to '' — never
    '.', which would poison every prefix query with a './' no path
    starts with."""
    try:
        rel = resolved.relative_to(Path(root).resolve()).as_posix()
    except ValueError:
        return ""
    return "" if rel == "." else rel


# -- write-path hooks (never raise — the write already landed) --------------

def add_path(root: Path, resolved: Path) -> None:
    rel = rel_under(root, resolved)
    if rel:
        for_root(root).add(rel)


def remove_path(root: Path, resolved: Path) -> None:
    rel = rel_under(root, resolved)
    if rel:
        for_root(root).remove(rel)


def refresh_path(root: Path, resolved: Path, exists_on_disk: bool) -> None:
    rel = rel_under(root, resolved)
    if rel:
        for_root(root).refresh(rel, exists_on_disk)


def rebuild(root: Path) -> int:
    return for_root(root).rebuild()


# ---------------------------------------------------------------------------
# Payload shaping (pure — operates on the index, never the filesystem)
# ---------------------------------------------------------------------------

def _paged(items: list, limit: int) -> tuple[list, bool]:
    return items[:limit], len(items) > limit


def ls_payload(index: _ProjectFileIndex, rel: str,
               limit: int = LS_CAP) -> tuple[dict, bool, Optional[str]]:
    """`ls`/`dir`'s result row: one directory level, names + kinds +
    file types + per-directory file counts — every field from the index."""
    if limit < 1:
        raise FileIndexError("validation", f"limit must be >= 1, got {limit}")
    index.ensure_built()
    paths = index.snapshot()
    if rel and index.is_file(rel):
        raise FileIndexError(
            "validation", f"'{rel}' is a file, not a directory",
            hint="get_meta serves a file's type/lines/signatures; "
                 "view_file serves its content",
        )
    prefix = (rel + "/") if rel else ""
    dirs: list[dict] = []
    files: list[dict] = []
    for name, is_dir in index.iter_children(prefix, paths):
        if is_dir:
            dirs.append({"name": name,
                         "files_within": index.count_under(prefix + name + "/", paths)})
        else:
            files.append({"name": name,
                          "file_type": file_type_for(Path(name).suffix)})
    shown_dirs, dirs_trunc = _paged(dirs, limit)
    shown_files, files_trunc = _paged(files, limit)
    truncated = dirs_trunc or files_trunc
    hint: Optional[str] = None
    if truncated:
        hint = (f"{limit} per list shown of {len(dirs)} dirs / {len(files)} files; "
                "raise limit, or file_hierarchy for the subtree's shape")
    elif not shown_dirs and not shown_files:
        hint = ("no files under this path in the index (empty dir, or everything "
                "below it is excluded by cie.extract.EXCLUDED_DIRS) — "
                "path_prefix checks a partial path, file_names_like searches names")
    return {
        "path": rel or ".",
        "dirs": shown_dirs, "dir_count": len(dirs),
        "files": shown_files, "file_count": len(files),
    }, truncated, hint


def hierarchy_payload(index: _ProjectFileIndex, rel: str, depth: int,
                      max_entries: int = TREE_ENTRY_CAP,
                      ) -> tuple[dict, bool, Optional[str]]:
    """`file_hierarchy`'s result row: an ASCII tree of the subtree with
    per-directory file counts, depth- and budget-limited with honest
    truncation notes — an agent orients in a repo's shape without
    walking it file-by-file via ls."""
    if depth < 1 or depth > 10:
        raise FileIndexError("validation", f"depth must be 1..10, got {depth}")
    if max_entries < 1:
        raise FileIndexError("validation", f"max_entries must be >= 1, got {max_entries}")
    index.ensure_built()
    paths = index.snapshot()
    if rel and index.is_file(rel):
        raise FileIndexError(
            "validation", f"'{rel}' is a file, not a directory",
            hint="get_meta serves a file's type/lines/signatures",
        )
    prefix = (rel + "/") if rel else ""

    lines: list[str] = [rel or "."]
    state = {"shown": 0, "truncated": False}

    def render(prefix: str, depth_left: int, indent: str) -> None:
        for name, is_dir in index.iter_children(prefix, paths):
            if state["shown"] >= max_entries:
                state["truncated"] = True
                lines.append(f"{indent}… (budget reached — raise max_entries)")
                return
            state["shown"] += 1
            pad = "  " * (indent.count("  ") + 1)
            if is_dir:
                count = index.count_under(prefix + name + "/", paths)
                lines.append(f"{pad}{name}/ ({count} files)")
                if depth_left > 1:
                    render(prefix + name + "/", depth_left - 1, pad)
            else:
                lines.append(f"{pad}{name}")

    render(prefix, depth, "")
    truncated = state["truncated"]
    hint: Optional[str] = None
    if truncated:
        hint = (f"budget reached: showing {state['shown']} of the subtree's "
                f"entries within depth {depth}; raise max_entries or narrow "
                "the path")
    elif state["shown"] == 0:
        hint = ("no files under this path in the index (empty dir, or excluded "
                "by cie.extract.EXCLUDED_DIRS)")
    return {
        "path": rel or ".",
        "depth": depth,
        "entries_shown": state["shown"],
        "tree": "\n".join(lines),
        "files_within": index.count_under(prefix, paths),
        "dirs_within": len(index.dirs_under(prefix, paths)),
    }, truncated, hint


def names_like_payload(index: _ProjectFileIndex, pattern: str,
                      limit: int = MATCH_CAP) -> tuple[dict, bool, Optional[str]]:
    """`file_names_like`'s result row: every indexed path matching an
    fnmatch glob ('*.py', '*test*', 'pay*') — the whole-repo name search."""
    if not pattern.strip():
        raise FileIndexError("validation", "pattern must not be empty")
    if limit < 1:
        raise FileIndexError("validation", f"limit must be >= 1, got {limit}")
    index.ensure_built()
    matches = fnmatch.filter(index.snapshot(), pattern)
    matches.sort()
    shown, truncated = _paged(matches, limit)
    hint: Optional[str] = None
    if truncated:
        hint = f"{limit} of {len(matches)} matches shown; raise limit for more"
    elif not matches:
        hint = ("no indexed path matches — fnmatch syntax: * any run, ? one "
                "char, [seq] a set; path_prefix completes a known partial "
                "path, file_hierarchy shows the tree to browse instead")
    return {
        "pattern": pattern,
        "matches": shown,
        "count": len(matches),
    }, truncated, hint


def prefix_payload(index: _ProjectFileIndex, prefix: str,
                   limit: int = MATCH_CAP) -> tuple[dict, bool, Optional[str]]:
    """`path_prefix`'s result row: everything under a PARTIAL path — the
    typo/half-remembered-path recovery tool, two bisects away in the
    sorted array. Zero matches come back with the nearest existing
    neighbors as a 'did you mean', so a wrong guess costs one turn, not
    a hunt."""
    if limit < 1:
        raise FileIndexError("validation", f"limit must be >= 1, got {limit}")
    index.ensure_built()
    paths = index.snapshot()
    prefix = prefix.strip().lstrip("./").rstrip("/")
    matches = [p for p in paths if p.startswith(prefix)] if prefix else paths
    # Dirs are the parents of the matched files, not dirs_under(prefix+"/")
    # — a PARTIAL segment prefix ('sr' matching 'src/...') has no
    # directory range of its own, but its matches' ancestors are exactly
    # the directories worth listing.
    dir_set: set[str] = set()
    for path in matches:
        parent = path
        while "/" in parent:
            parent = parent.rsplit("/", 1)[0]
            dir_set.add(parent)
    dirs = sorted(dir_set)
    shown_files, files_trunc = _paged(matches, limit)
    shown_dirs, dirs_trunc = _paged(dirs, limit)
    truncated = files_trunc or dirs_trunc
    hint: Optional[str] = None
    if not matches and not dirs:
        lo = bisect_left(paths, prefix)
        neighbors = [p for p in (paths[lo - 1] if lo else None,
                                paths[lo] if lo < len(paths) else None) if p]
        hint = (f"nothing starts with '{prefix}'"
                + (f"; nearest indexed paths: {', '.join(neighbors[:2])}" if neighbors
                   else " (the index is empty — did cie ever index this root?)"))
    elif truncated:
        hint = (f"{limit} per list shown of {len(matches)} files / {len(dirs)} dirs; "
                "raise limit or narrow the prefix")
    return {
        "prefix": prefix,
        "files": shown_files, "file_count": len(matches),
        "dirs": shown_dirs, "dir_count": len(dirs),
    }, truncated, hint