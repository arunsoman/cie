"""Shared source re-analysis for CI-01/02/06/07: clone detection and
performance analysis both need the ACTUAL tree-sitter subtree for a
FUNC/METHOD graph node, which extract.py never stores in the graph — the
schema only keeps signature/docstring/line-range, never bodies (see
`Neo4jRepository.get_file_skeleton`'s docstring: "never bodies"). Both
passes re-parse the source file (once per file, cached by `SourceIndex`)
and re-locate each graph node's tree-sitter subtree by its declared
`line_start` — this module owns that shared "graph node -> tree-sitter
node" bridge plus the token/AST-shape and loop/recursion primitives both
passes build on, so neither reimplements tree-sitter traversal
independently.

This is genuinely a SECOND parse of every analyzed file (the first
parse, in `extract.py`, happens at load/reindex time and its tree is
never kept around — keeping every project's full parse trees resident
just in case a later analysis pass wants them would be a much bigger
memory cost than re-parsing on the rare, explicit, on-demand analysis
call this module serves). `cie.clone_detect`/`cie.perf_analyze` are
scheduled/batch passes (spec section 13.5's own framing), not part of
the `reindex_file` hot path, so this tradeoff is the right one here.
"""
from __future__ import annotations

from pathlib import Path
from typing import Optional

from cie import extract

# Loop constructs, per language — tree-sitter type names.
_LOOP_TYPES = {
    "for_statement", "for_in_statement", "while_statement", "do_statement",
    "enhanced_for_statement",  # java
}

# A nested function/lambda/method defines its OWN loop-nesting and
# recursion scope, not a continuation of the enclosing one — walks below
# stop descending into these (still visited for their own top-level
# lookup elsewhere, just not folded into the ENCLOSING function's depth).
_SCOPE_BOUNDARY_TYPES = extract._FUNCTION_TYPES  # noqa: SLF001


class SourceIndex:
    """Per-run cache of parsed files, keyed by path string.

    A file is parsed at most once per `SourceIndex` instance no matter
    how many graph nodes from it are looked up — `clone_detect`/
    `perf_analyze` both build one instance and reuse it across every
    node in the project being analyzed.
    """

    def __init__(self) -> None:
        self._trees: dict[str, Optional[tuple]] = {}

    def _tree_for(self, source_file: str):
        if source_file not in self._trees:
            self._trees[source_file] = extract.parse_file(Path(source_file))
        return self._trees[source_file]

    def locate(self, source_file: str, line_start: int, kind: str):
        """Return the tree-sitter node for a graph FUNC/METHOD/CLASS node
        at `line_start` (1-based, matching `extract._location`'s own
        convention), or `None` when the file can't be re-parsed or no
        matching declaration is found at that exact line — the source
        changed since the graph was last loaded/reindexed (a stale
        graph, not a bug in this lookup)."""
        parsed = self._tree_for(source_file)
        if parsed is None:
            return None
        tree, _source = parsed
        predicate = extract.is_class_node if kind == "class" else extract.is_function_node
        found = None

        def _walk(node) -> None:
            nonlocal found
            if found is not None:
                return
            if predicate(node) and node.start_point[0] + 1 == line_start:
                found = node
                return
            for child in node.children:
                _walk(child)

        _walk(tree.root_node)
        return found


def leaf_tokens(node) -> list[str]:
    """Every leaf token's literal text within `node`'s span, in source
    order — the raw shape CI-01's token-level clone signal compares.
    Exact text, so a renamed variable breaks the match; that's the
    point — it's what makes this a SEPARATE signal from CI-02's
    structural shape below, which normalizes exactly that away."""
    out: list[str] = []

    def _walk(n) -> None:
        if not n.children:
            text = n.text.decode("utf-8", errors="replace").strip()
            if text:
                out.append(text)
            return
        for child in n.children:
            _walk(child)

    _walk(node)
    return out


def structural_shape(node) -> list[str]:
    """Every node TYPE (never text) within `node`'s span, in preorder —
    CI-02's structural signal. Two functions with identical control-flow
    shape but completely different identifier/literal names produce the
    SAME sequence here, which is exactly what makes this signal catch
    what CI-01's literal-token signal misses."""
    out: list[str] = []

    def _walk(n) -> None:
        out.append(n.type)
        for child in n.children:
            _walk(child)

    _walk(node)
    return out


def loop_nesting_depth(node) -> int:
    """Max nesting depth of loop constructs directly within `node`'s own
    scope. Does not descend into a nested function/lambda's body — that
    has its own independent nesting depth (see `_SCOPE_BOUNDARY_TYPES`);
    counting it here would attribute a helper's loops to its caller."""
    best = 0

    def _walk(n, depth: int, is_root: bool) -> None:
        nonlocal best
        for child in n.children:
            if not is_root and child.type in _SCOPE_BOUNDARY_TYPES:
                continue  # nested function: separate scope, don't descend
            child_depth = depth + 1 if child.type in _LOOP_TYPES else depth
            best = max(best, child_depth)
            _walk(child, child_depth, is_root=False)

    _walk(node, 0, is_root=True)
    return best


def loop_nodes(node) -> list:
    """Every loop-construct node directly within `node`'s own scope
    (same scope-boundary rule as `loop_nesting_depth`) — used by
    anti-pattern detection to inspect each loop's BODY independently
    (e.g. "is there a call inside this specific loop")."""
    out: list = []

    def _walk(n, is_root: bool) -> None:
        for child in n.children:
            if not is_root and child.type in _SCOPE_BOUNDARY_TYPES:
                continue
            if child.type in _LOOP_TYPES:
                out.append(child)
            _walk(child, is_root=False)

    _walk(node, is_root=True)
    return out


def calls_name(node, name: str) -> bool:
    """True if `node`'s subtree contains a call whose callable text is
    (or ends with, for `self.name`/`this.name`/`mod.name`) `name` —
    CI-06's crude but real recursion signal. Same scope-boundary rule as
    `loop_nesting_depth`: a nested function calling `name` is that
    function's own possible recursion, not this one's."""
    target = f".{name}"
    found = False

    def _walk(n, is_root: bool) -> None:
        nonlocal found
        if found:
            return
        for child in n.children:
            if not is_root and child.type in _SCOPE_BOUNDARY_TYPES:
                continue
            if extract.is_call_node(child):
                fn = child.child_by_field_name("function")
                if fn is not None:
                    text = fn.text.decode("utf-8", errors="replace")
                    if text == name or text.endswith(target):
                        found = True
                        return
            _walk(child, is_root=False)

    _walk(node, is_root=True)
    return found


def call_names_in(node) -> list[str]:
    """Every call's callable text within `node`'s span (including nested
    scopes — unlike the functions above, anti-pattern detection over a
    LOOP BODY wants everything reachable, not just the loop's own
    top-level scope, since a loop calling a locally-defined closure that
    itself makes a DB call is still an N+1 shape worth flagging)."""
    out: list[str] = []

    def _walk(n) -> None:
        if extract.is_call_node(n):
            fn = n.child_by_field_name("function")
            if fn is not None:
                out.append(fn.text.decode("utf-8", errors="replace"))
        for child in n.children:
            _walk(child)

    _walk(node)
    return out
