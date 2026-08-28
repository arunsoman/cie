"""RQ-12: Diff-Aware Queries.

The spec frames this as depending on DM-05 (temporal versioning:
`valid_from`/`valid_to` history kept IN the graph) — that dependency
does not exist in this codebase (`load_extraction`/`reindex_file` both
REPLACE, never version, a project's nodes; DM-05 itself is still
"Missing" in the spec's own table). Rather than block RQ-12 on building
temporal storage first (a much larger, separate undertaking), this
module answers the same question — "what changed?" — a DIFFERENT way:
a PURE, in-memory diff between two SEPARATE `extract.extract_tree` runs
over two directories (two git worktrees, two commits checked out to two
paths, or a before/after of the same tree). Nothing is read from or
written to Neo4j here; `graph_diff` never touches the live graph at all.

Node ids embed the absolute source path (`extract.py`'s
`f"{file_id}::{name}@{line}"`, `file_id = str(path)`), so comparing two
DIFFERENT roots' raw ids would treat every symbol as "added" and
"removed" even when nothing changed — the SAME symbol in the SAME
relative file just has a different absolute-path prefix. Diffing here
re-keys every node by `(relative_file_path, label, kind)` instead, which
is stable across two different roots for exactly this reason.
"""
from __future__ import annotations

from pathlib import Path
from typing import Optional

from cie.extract import extract_tree

# Node fields whose change makes a same-key node "modified" rather than
# unchanged — line-number drift alone (a symbol just moved down the file
# because something above it grew) is deliberately NOT one of these,
# it would make "modified" fire on nearly every symbol in a touched file.
_MODIFIED_FIELDS = ("signature", "docstring")


def _relative_key(node: dict, root: Path) -> tuple[str, str, str]:
    """`(relative_source_file, label, kind)` — stable across two
    different roots for the same logical symbol."""
    source_file = node.get("source_file", "")
    try:
        rel = str(Path(source_file).resolve().relative_to(root.resolve()))
    except (ValueError, OSError):
        # Not under root (shouldn't happen for extract_tree's own output,
        # but never let a path-resolution edge case crash the diff) —
        # fall back to the raw string so the node still participates in
        # the comparison instead of being silently dropped.
        rel = source_file
    return (rel, node.get("label", ""), node.get("kind", ""))


def graph_diff(before_root: Path, after_root: Path) -> dict:
    """Diff two directory trees' extractions. Returns
    `{"added": [...], "removed": [...], "modified": [...]}`, each a list
    of node dicts (`modified` entries carry BOTH `before`/`after` under
    those keys, plus `changed_fields`). Unsupported/unreadable files are
    skipped the same way `extract_tree` already skips them (a warning on
    stderr, not a raised error) — a diff over a tree with one bad file
    still reports every other file's real changes.
    """
    before_root, after_root = Path(before_root), Path(after_root)
    before_nodes = {
        _relative_key(n, before_root): n for n in extract_tree(before_root).nodes
    }
    after_nodes = {
        _relative_key(n, after_root): n for n in extract_tree(after_root).nodes
    }

    before_keys = set(before_nodes)
    after_keys = set(after_nodes)

    added = [after_nodes[k] for k in sorted(after_keys - before_keys)]
    removed = [before_nodes[k] for k in sorted(before_keys - after_keys)]

    modified: list[dict] = []
    for key in sorted(before_keys & after_keys):
        b, a = before_nodes[key], after_nodes[key]
        changed = [f for f in _MODIFIED_FIELDS if b.get(f, "") != a.get(f, "")]
        if changed:
            modified.append({"before": b, "after": a, "changed_fields": changed})

    return {"added": added, "removed": removed, "modified": modified}


def graph_diff_summary(diff: dict) -> dict:
    """Compact counts-only view of a `graph_diff` result — the shape a
    caller wanting "how much changed" (not the full detail) wants, e.g.
    a CI comment or a one-line status."""
    return {
        "added": len(diff["added"]),
        "removed": len(diff["removed"]),
        "modified": len(diff["modified"]),
    }
