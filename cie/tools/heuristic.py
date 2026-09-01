"""Heuristic (text/AST-index) tool implementations, backed by a
`cie.tools.index.SymbolIndex` — no database, no network.

Used internally by `cie.tools.ToolService` as its fallback when the graph
is empty or a query fails (see that class's `_heuristic_fallback`). Also
used directly by forge/tools.py's `LocalDiskBackend` — that class used to
carry its OWN independent copy of this same logic; it now delegates here
instead (its own docstring has the full history, including an earlier,
reverted attempt to delete it outright — `make_tools()`'s `preference`
default of `"local"`, and several tests using `tools_pref="local"`
explicitly, are genuine no-Neo4j-required callers, not vestigial). This
module has zero Neo4j dependency at import time, so `LocalDiskBackend` can
depend on it without needing a live database — only the `cie` package
needs to be importable, which it always is (forge and cie ship together,
see `pyproject.toml`'s `[tool.setuptools] packages` list).

Confidence is always INFERRED (or EXTRACTED only where noted) — this is a
text/regex scan, not the real graph; callers should weight results
accordingly. Bug fixes made during the 2026-08-07 move to this module
(all confirmed against the pre-move code, not hypothetical):

- `callers()`: the old per-suffix `if len(hits) >= LIST_CAP: break` only
  exited the inner per-file loop, not the outer per-suffix loop — a hit in
  every one of the 6 suffixes could each independently fill past LIST_CAP
  before the (missing) final slice, so results were effectively unbounded.
- `callers()`'s definition-line exclusion used a `def|function` keyword
  regex — which can never cover Java at all, since a Java method
  definition (`public void target() {`) has no such literal keyword
  token in real source. Fixed by checking the already-built index for
  known (file, line) definition locations instead of pattern-matching a
  per-language keyword list.
- `path_between()`'s adjacency-building regex only matched
  `def|function|class` callee signatures — a Java method's signature (bare
  `name(params)`, no prefix — see SymbolIndex._parse_java) fell through to
  using the WHOLE signature string as the graph key, breaking BFS
  adjacency for any Java callee.
- `affected_by()` always reported `truncated=False` even when its own
  `results[:LIST_CAP]` slice had just discarded real hits.
"""

from __future__ import annotations

import re
from collections import deque
from pathlib import Path
from typing import Optional

from cie.tools.index import INDEXABLE_SUFFIXES, LIST_CAP, Symbol, SymbolIndex

VIEW_WINDOW = 100

#: Definition-line keywords across every language SymbolIndex parses
#: (python def/class, js/ts function/class, java's bare method has no
#: keyword of its own but its enclosing class does) — used to exclude a
#: symbol's OWN definition line from matching as a "call" of itself.
_DEFINITION_KEYWORDS = r"(?:def|function|class|method)"

#: `search_symbol`'s only real `kind` values here — this class is backed
#: by `cie.tools.index.SymbolIndex`, whose own `Symbol.kind` type comment
#: says it plainly: `# function | class | method`. NOT the same
#: vocabulary as `cie.models.NodeKind` (the persisted-graph engine this
#: class exists as a fallback for) — this index never sees the graph's
#: other kinds at all. Guarded independently of `cie.tools.ToolService`'s
#: own equivalent guard because this class has real external callers of
#: its own (see this module's docstring — forge/tools.py's
#: `LocalDiskBackend`), not just ToolService's internal fallback.
_VALID_SYMBOL_KINDS = {"function", "class", "method"}

#: Same unvalidated-enum shape as `kind` above, in `affected_by` below:
#: any value other than exactly "incoming" silently became "outgoing"
#: (`neighbor_key = "caller_file" if direction == "incoming" else
#: "callee_file"`), with no signal the input wasn't understood.
_VALID_DIRECTIONS = {"incoming", "outgoing"}


def _invalid_kind_hint(kind: str) -> Optional[str]:
    if kind and kind not in _VALID_SYMBOL_KINDS:
        return (f"kind={kind!r} is not a recognized filter (valid: "
                f"{', '.join(sorted(_VALID_SYMBOL_KINDS))}, or '' for any).")
    return None


def _invalid_direction_hint(direction: str) -> Optional[str]:
    if direction not in _VALID_DIRECTIONS:
        return (f"direction={direction!r} is not a recognized value (valid: "
                f"{', '.join(sorted(_VALID_DIRECTIONS))}) — nothing was silently "
                "assumed; pass one of these explicitly.")
    return None


def _envelope(tool: str, results: list, hint: Optional[str] = None,
              truncated: bool = False, total: Optional[int] = None, ok: bool = True) -> dict:
    return {"ok": ok, "tool": tool, "results": results, "truncated": truncated,
            "total": total if total is not None else len(results), "hint": hint}


def _signature_symbol_name(signature: str) -> str:
    """Bare symbol name out of a Symbol.signature string, across every
    language SymbolIndex produces: `def foo(...)`/`function foo(...)`/
    `class Foo` (python/js/ts) all carry a keyword prefix; a Java method's
    signature is bare (`foo(params)`, no prefix — see
    SymbolIndex._parse_java), so that case falls back to the leading
    identifier instead of requiring one of the other keywords."""
    m = re.search(rf"{_DEFINITION_KEYWORDS}\s+(\w+)", signature)
    if m:
        return m.group(1)
    m = re.search(r"^(\w+)", signature)
    return m.group(1) if m else signature


class HeuristicToolSet:
    """Read-only heuristic queries over a `SymbolIndex`. Genuinely useful
    on its own (a mini-cie) — `callers`/`callees`/`path_between`/
    `affected_by` are text+index heuristics, confidence INFERRED, which a
    repair loop weighting confidence already knows to discount."""

    def __init__(self, project_dir: Path, index: SymbolIndex):
        self.project_dir = Path(project_dir)
        self.index = index

    # -- reading ------------------------------------------------------------
    def view_file(self, path: str, start: int = 1, end: int = VIEW_WINDOW) -> dict:
        f = self.project_dir / path
        if not f.exists():
            return _envelope("view_file", [], ok=False,
                             hint=f"no such file: {path}; try file_skeleton on its directory or search_symbol")
        lines = f.read_text(errors="replace").splitlines()
        total = len(lines)
        start = max(1, start); end = min(end or VIEW_WINDOW, total, start + VIEW_WINDOW - 1)
        window = "\n".join(f"{i:>6}\t{lines[i-1]}" for i in range(start, end + 1))
        skeleton = [s.as_hit() for s in self.index.in_file(path)]
        hint = None if end >= total else f"window {start}-{end} of {total}; call again with start={end+1}"
        return _envelope("view_file", [{
            "path": path, "total_lines": total,
            "window": {"start": start, "end": end}, "content": window,
            "symbol_index": skeleton}], hint=hint)

    def file_skeleton(self, path: str) -> dict:
        f = self.project_dir / path
        if not f.exists():
            return _envelope("file_skeleton", [], ok=False, hint=f"no such file: {path}")
        syms = self.index.in_file(path)
        if not syms:
            return _envelope("file_skeleton", [{"path": path, "symbols": []}],
                             hint="no symbols indexed (non-code file or unsupported language)")
        return _envelope("file_skeleton", [{"path": path, "symbols": [s.as_hit() for s in syms]}])

    def search_symbol(self, name: str, kind: str = "") -> dict:
        bad_kind_hint = _invalid_kind_hint(kind)
        if bad_kind_hint:
            return _envelope("search_symbol", [], hint=bad_kind_hint)
        hits = self.index.find(name, kind)[:LIST_CAP]
        if not hits:
            return _envelope("search_symbol", [],
                             hint=f"no symbol named '{name}'; try a shorter substring or different kind")
        conf = "EXTRACTED" if any(s.file.endswith(".py") for s in hits) else "INFERRED"
        return _envelope("search_symbol", [s.as_hit(confidence=conf) for s in hits])

    # -- graph-ish queries (heuristic) --------------------------------------
    def callers(self, symbol: str) -> dict:
        pat = re.compile(rf"\b{re.escape(symbol)}\s*\(")
        # (file, line) of every definition of `symbol` per the already-
        # built index, so a definition line is never counted as its own
        # caller — checked against the index directly rather than a
        # per-language keyword regex (a Java method definition, e.g.
        # `public void target() {`, has no literal "def"/"function"/
        # "method" token in real source at all, unlike python/js/ts, so a
        # keyword-based exclusion can never cover every language; the
        # index already knows exactly where every definition is).
        definition_lines = {
            (s.file, s.start) for s in self.index.symbols if s.name == symbol
        }
        hits = []
        truncated = False
        for suffix in INDEXABLE_SUFFIXES:
            if truncated:
                break
            for f in sorted(self.project_dir.rglob(f"*{suffix}")):
                if any(p.startswith(".") or p in ("node_modules", "__pycache__") for p in f.parts):
                    continue
                rel = str(f.relative_to(self.project_dir))
                for i, line in enumerate(f.read_text(errors="replace").splitlines(), start=1):
                    if pat.search(line) and (rel, i) not in definition_lines:
                        enclosing = self._enclosing(rel, i)
                        hits.append({"caller_file": rel, "line": i,
                                     "caller_signature": enclosing.signature if enclosing else "(module level)",
                                     "relation": "calls", "confidence": "INFERRED"})
                        break  # one hit per file is enough for localization
                if len(hits) >= LIST_CAP:
                    truncated = True
                    break
        if not hits:
            return _envelope("callers", [],
                             hint=f"no callers of '{symbol}' found (heuristic scan; empty ≠ unused)")
        return _envelope("callers", hits[:LIST_CAP], truncated=truncated,
                         hint=f"results capped at {LIST_CAP}" if truncated else None)

    def callees(self, symbol: str) -> dict:
        defs = self.index.find(symbol)
        if not defs:
            return _envelope("callees", [], hint=f"no definition of '{symbol}' in index")
        sym = defs[0]
        f = self.project_dir / sym.file
        body = "\n".join(f.read_text(errors="replace").splitlines()[sym.start - 1: sym.end])
        hits = []
        for s in self.index.symbols:
            if s.name != symbol and re.search(rf"\b{re.escape(s.name)}\s*\(", body):
                hits.append({"callee_file": s.file, "callee_signature": s.signature,
                             "relation": "calls", "confidence": "INFERRED"})
        truncated = len(hits) > LIST_CAP
        return _envelope("callees", hits[:LIST_CAP], truncated=truncated,
                         hint=f"results capped at {LIST_CAP}" if truncated else None)

    def path_between(self, a: str, b: str) -> dict:
        """BFS over the heuristic calls graph, symbol → symbol.

        SF7: adjacency is built LAZILY — `callees()` is only called for a
        node once it's actually dequeued from the BFS frontier, and BFS
        still stops the instant `b` is found. This used to eagerly call
        `callees()` for EVERY indexed symbol up front (each `callees()`
        call itself re-reads and regex-scans a file against every other
        symbol name), materializing the whole project's O(symbols^2)
        adjacency graph before BFS even started — regardless of how close
        `a`/`b` actually are, or whether the target is reachable at all.
        This function has no `max_hops` parameter (nor does its one
        caller, `cie/tools/__init__.py`'s heuristic fallback, ever pass
        one) — unbounded, matching the prior behavior exactly."""
        queue = deque([(a, [a])]); seen = {a}
        while queue:
            node, path = queue.popleft()
            if node == b:
                chain = []
                for name in path:
                    defs = self.index.find(name)
                    chain.append({"symbol": name,
                                  "file": defs[0].file if defs else "?",
                                  "confidence": "INFERRED"})
                return _envelope("path_between", [{"hops": len(path) - 1, "chain": chain}])
            for hit in self.callees(node)["results"]:
                nxt = _signature_symbol_name(hit["callee_signature"])
                if nxt not in seen:
                    seen.add(nxt); queue.append((nxt, path + [nxt]))
        return _envelope("path_between", [], hint=f"no call path from {a} to {b} within index")

    def affected_by(self, file_path: str, max_depth: int = 3,
                    direction: str = "incoming") -> dict:
        """Local BFS approximation of blast radius: seeded from every symbol
        defined in `file_path`, BFS outward over the same heuristic
        caller/callee text-scan `callers()`/`callees()` already use —
        direction="incoming" (default) follows callers (what depends on
        this file); "outgoing" follows callees (what this file depends on).
        Confidence is always INFERRED, same caveat as callers/callees."""
        bad_direction_hint = _invalid_direction_hint(direction)
        if bad_direction_hint:
            return _envelope("affected_by", [], hint=bad_direction_hint)
        seeds = [s.name for s in self.index.in_file(file_path)]
        if not seeds:
            return _envelope("affected_by", [], ok=False,
                             hint=f"no symbols indexed for '{file_path}'; try file_skeleton or reindex")
        seen_files = {file_path}
        visited_symbols = set(seeds)
        results: list[dict] = []
        queue = deque((name, 0) for name in seeds)
        neighbor_key = "caller_file" if direction == "incoming" else "callee_file"
        truncated = False
        while queue:
            if len(results) >= LIST_CAP:
                truncated = bool(queue)
                break
            name, dist = queue.popleft()
            if dist >= max_depth:
                continue
            neighbors = self.callers(name) if direction == "incoming" else self.callees(name)
            for hit in neighbors["results"]:
                neighbor_file = hit.get(neighbor_key)
                if not neighbor_file:
                    continue
                if neighbor_file not in seen_files:
                    seen_files.add(neighbor_file)
                    results.append({"distance": dist + 1, "file": neighbor_file,
                                    "confidence": hit.get("confidence", "INFERRED")})
                for sym in self.index.in_file(neighbor_file):
                    if sym.name not in visited_symbols:
                        visited_symbols.add(sym.name)
                        queue.append((sym.name, dist + 1))
        if not results:
            return _envelope("affected_by", [],
                             hint=f"no files found affected by '{file_path}' within {max_depth} hops")
        results.sort(key=lambda r: r["distance"])
        truncated = truncated or len(results) > LIST_CAP
        hint = "distance-1 hits are the most directly affected"
        if truncated:
            hint += f"; results capped at {LIST_CAP}"
        return _envelope("affected_by", results[:LIST_CAP], truncated=truncated, hint=hint)

    def failing_context(self, test: str) -> dict:
        """From a failing test file (optionally 'path::test_name'), rank the
        symbols it touches by graph distance.

        `test` is usually a real relative file path (pytest/vitest's own
        node-id convention). Java/JUnit test runners never print one —
        confirmed by running a real `gradle test` against a deliberately
        failing test: Gradle's console reporter emits a bare
        "ClassName > methodName() FAILED" summary, no path at all — so a
        `test` value that isn't a real path is resolved as a bare class
        name against the symbol index instead, before giving up."""
        test_path = test.split("::")[0]
        # ''/blank and directory-valued inputs resolved to the project_root
        # itself (Path('') is cwd; a root IS 'exists') and then crashed on
        # read_text — found live by R5's unavailable-bucket scan. A blank
        # test identifier is a validation miss, not a crash.
        if not test_path:
            return _envelope(
                "failing_context", [], ok=False,
                hint="empty test identifier; pass the test file path "
                     "(optionally 'path::test_name') from the failing test run",
            )
        f = self.project_dir / test_path
        if not f.exists():
            by_name = next((s for s in self.index.symbols
                            if s.name == test_path and s.kind == "class"), None)
            if by_name is not None:
                test_path = by_name.file
                f = self.project_dir / test_path
        if not f.is_file():
            hint = (
                f"no such test file: {test_path}"
                if not f.exists()
                else f"'{test_path}' is a directory; pass the test FILE path"
            )
            return _envelope("failing_context", [], ok=False, hint=hint)
        text = f.read_text(errors="replace")
        results, seen = [], set()
        # distance 1: symbols directly referenced in the test
        for s in self.index.symbols:
            if (s.file.startswith("tests/") or s.file.startswith("src/test/")
                    or s.file.endswith("Test.java") or s.file.endswith("Tests.java")
                    or s.file.endswith(".spec.ts")):
                continue
            if re.search(rf"\b{re.escape(s.name)}\b", text) and s.name not in seen:
                seen.add(s.name)
                results.append({"distance": 1, "file": s.file, "symbol": s.name,
                                "signature": s.signature, "confidence": "EXTRACTED"})
        # distance 2: callees of distance-1 symbols
        for r in list(results):
            for hit in self.callees(r["symbol"])["results"]:
                name = _signature_symbol_name(hit["callee_signature"])
                if name and name not in seen:
                    seen.add(name)
                    results.append({"distance": 2, "file": hit["callee_file"], "symbol": name,
                                    "signature": hit["callee_signature"], "confidence": "INFERRED"})
        results.sort(key=lambda r: r["distance"])
        if not results:
            return _envelope("failing_context", [],
                             hint="test references no indexed symbols; localize via traceback instead")
        return _envelope("failing_context", results[:LIST_CAP],
                         hint="distance-1 symbols are the prime suspects")

    # -- helpers --------------------------------------------------------------
    def _enclosing(self, rel: str, line: int) -> Optional[Symbol]:
        best = None
        for s in self.index.in_file(rel):
            if s.start <= line <= s.end and (best is None or s.start > best.start):
                best = s
        return best
