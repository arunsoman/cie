"""DM-14: explicit `TESTS` edges (test symbol -> implementation symbol).

A separate module from `cie.callgraph` rather than overloading it — the
resolution problem is genuinely different (matching a TEST's NAME/decorator
against an IMPLEMENTATION's name, not resolving a call site through an
import map) even though it reuses the same `_Index` (built by
`callgraph._Index`) for its symbol/class dictionaries and Python-module
resolution. Runs as its own pass, AFTER `resolve_call_edges` (its output is
one of this pass's three input signals — see below), given the same
per-file `Extraction` list plus that pass's resolved `calls` edges.

Four heuristics — (1)-(3) match the confirmed doc's three documented
signals; (4) was added 2026-08-31 by the dogfooding pass (see below):

1. Naming convention: `test_foo` -> a function/method literally named
   `foo`; a method `test_bar` declared inside a class named `TestFoo`/
   `FooTest`/`FooTestCase` -> either a function/method named `bar` OR a
   class named `Foo` (both checked; either can match independently).
   INFERRED confidence — a name match alone is a guess, not proof.
2. Confidence upgrade: when a naming match is ALSO backed by a resolved
   `calls` edge from that same test node to that same target (the test
   function actually calls the thing its name suggests it tests), the
   edge is upgraded to EXTRACTED — the test provably exercises that
   symbol, not just its name.
3. `@patch("module.path.Target")` / `@mock.patch(...)` decorator
   `args_text` (already captured per-node by `extract.py`'s
   `_decorators_json` — Python decorator extraction was added alongside
   this module specifically because `@patch` is a Python-only convention
   and decorator capture used to be TS/Java-only): the patched dotted
   path is resolved to a node id via the SAME Python-module-resolution
   `_Index._resolve_python_module` already provides for imports. Always
   EXTRACTED — an explicit textual reference, not a name-similarity guess.
4. Direct calls (2026-08-31): every resolved `calls` edge from a test
   symbol to a symbol defined in PRODUCTION code (non-test file,
   FUNC/METHOD/CLASS) creates a TESTS edge, EXTRACTED. Rationale, found
   by dogfooding cie on itself: cie's own 308-test suite resolved
   exactly ONE TESTS edge under (1)-(3) — modern suites use behavioral
   names (`test_mcp_auto_serves_an_existing_index`, pytest best
   practice), which starve the name gate, and `monkeypatch` instead of
   `@patch`, which starves the decorator gate. A resolved calls edge is
   the same proof standard (2) upgrades on, so it now creates the edge
   directly. Tests calling helpers defined in test files or in
   `conftest.py` still get no edge (those are test infrastructure,
   not production code).

What "test symbol" means: a FUNC/METHOD node whose file matches a test
glob (`test_*.py`, `*_test.py`, `*.test.ts`, `*.test.tsx`, `*.spec.ts`,
`*.spec.tsx`) OR whose own label starts with `test`/`test_` by naming
convention (OR, not AND — either signal alone qualifies a node as a test
symbol worth resolving TESTS edges from).

Not exhaustive by design: a test with an unconventional name that
doesn't DIRECTLY call its target and doesn't patch it by dotted string
path (e.g. it only reaches it through a helper) still gets no TESTS
edge, same as any heuristic system. (4) closed the biggest measured
gap — behavioral names — not all of them, and that's the intended
scope.
"""
from __future__ import annotations

import fnmatch
import json
import posixpath
import re
from collections import defaultdict
from typing import Optional

from cie.callgraph import _Index, _file_hub_id
from cie.extract import Extraction
from cie.models import Confidence, NodeKind

# Test-file glob patterns, matched against the file's own basename (not
# its full path) so a nested `src/feature/test_foo.py` still counts.
_TEST_FILE_GLOBS = (
    "test_*.py", "*_test.py",
    "*.test.ts", "*.test.tsx",
    "*.spec.ts", "*.spec.tsx",
)

# Decorator names this module recognizes as a mock-patch call whose first
# string argument is a dotted target path. `patch.object(Cls, "method")`'s
# first argument is an object reference, not a dotted string, so it is
# deliberately NOT included here — heuristic (c) only covers the
# string-dotted-path form the spec names.
_PATCH_DECORATOR_NAMES = {"patch", "mock.patch"}

_STRING_LITERAL_RE = re.compile(r"""['"]([^'"]+)['"]""")


def _is_test_file(source_file: str) -> bool:
    name = posixpath.basename((source_file or "").replace("\\", "/"))
    return any(fnmatch.fnmatch(name, glob) for glob in _TEST_FILE_GLOBS)


def _is_production_file(source_file: str) -> bool:
    """Heuristic (4)'s target gate: production code, i.e. NOT a test-glob
    file AND NOT `conftest.py` — a conftest is test infrastructure
    (fixtures/helpers), not implementation, so tests calling into it
    must not mint TESTS edges. (`conftest.py` deliberately lives HERE,
    not in `_TEST_FILE_GLOBS`: that set gates which nodes count as test
    SOURCES, and a `test_*` helper inside a conftest still qualifies as
    a test symbol by name — this gate is only about valid TARGETS.)"""
    name = posixpath.basename((source_file or "").replace("\\", "/"))
    return not _is_test_file(source_file) and name != "conftest.py"


def _is_test_symbol_name(label: str) -> bool:
    return (label or "").lower().startswith("test")


def _impl_name_candidates(label: str) -> list[str]:
    """`test_foo` -> `["foo"]`; `testFoo`/`TestFoo` (as a FUNCTION/METHOD
    label, not a class) -> `["Foo"]`; anything not test-prefixed -> `[]`."""
    if label.startswith("test_") and len(label) > len("test_"):
        return [label[len("test_"):]]
    if label.lower().startswith("test") and len(label) > len("test"):
        return [label[len("test"):]]
    return []


def _class_name_candidates(label: str) -> list[str]:
    """`TestFoo` -> `["Foo"]`; `FooTestCase` -> `["Foo"]`; `FooTest` ->
    `["Foo"]` — the three conventional test-class-name shapes."""
    out: list[str] = []
    if label.startswith("Test") and len(label) > len("Test"):
        out.append(label[len("Test"):])
    if label.endswith("TestCase") and len(label) > len("TestCase"):
        out.append(label[: -len("TestCase")])
    elif label.endswith("Test") and len(label) > len("Test"):
        out.append(label[: -len("Test")])
    return [c for c in out if c]


def _patch_targets(decorators_json: str) -> list[str]:
    """Dotted target paths from every `@patch(...)`/`@mock.patch(...)`
    decorator on one node's `decorators` JSON string (see
    `extract.py::_decorators_json`)."""
    try:
        decorators = json.loads(decorators_json or "[]")
    except (TypeError, ValueError):
        return []
    out: list[str] = []
    for dec in decorators:
        if not isinstance(dec, dict) or dec.get("name") not in _PATCH_DECORATOR_NAMES:
            continue
        match = _STRING_LITERAL_RE.search(dec.get("args_text", "") or "")
        if match:
            out.append(match.group(1))
    return out


def _resolve_patch_target(index: _Index, dotted: str) -> Optional[str]:
    """Resolve a `@patch("a.b.Target")`/`@patch("a.b.Class.method")`
    dotted path to a node id, by probing progressively shorter module
    prefixes against the project's known extracted files (reusing
    `_Index._resolve_python_module`, the same resolver `import_map_for`
    already uses for `from a.b import X`) until one resolves, then
    looking up the remaining trailing segment(s) as a bare symbol or a
    `Class.method` pair in that file. Returns None when no prefix split
    resolves to a known file, or the file doesn't define that symbol —
    unlike DM-08's unresolved bases, a TESTS edge with no clear target is
    simply not created (see module docstring: not worth a stub node for
    an edge kind that already degrades gracefully to "no relationship
    found")."""
    parts = dotted.split(".")
    if len(parts) < 2:
        return None
    for split in range(len(parts) - 1, 0, -1):
        module = ".".join(parts[:split])
        remainder = parts[split:]
        target_file = index._resolve_python_module(module, "")  # noqa: SLF001
        if target_file is None:
            continue
        if len(remainder) == 1:
            hit = index.symbols.get(target_file, {}).get(remainder[0])
            if hit is not None:
                return hit
        else:
            cls_name, method_name = remainder[0], remainder[1]
            hit = index.class_methods.get(target_file, {}).get(cls_name, {}).get(method_name)
            if hit is not None:
                return hit
            hit = index.classes.get(target_file, {}).get(cls_name)
            if hit is not None:
                return hit
    return None


def resolve_test_edges(per_file: list[Extraction], call_edges: list[dict]) -> list[dict]:
    """Resolve DM-14 `TESTS` edges from per-file extractions.

    Args:
        per_file: one Extraction per source file (same shape
            `resolve_call_edges`/`resolve_inheritance_edges` take).
        call_edges: this run's already-resolved `calls` edges (see
            `resolve_call_edges`) — heuristic (2)'s confidence-upgrade
            signal. Passing the WHOLE project's calls edges (not just
            this file's) matters: a test can call a helper that itself
            calls the real target, but heuristic (2) only looks one hop
            (test -> target directly), so this is a deliberate, simple
            scope, not a missed transitive case.

    Returns:
        Edge dicts ``{source, target, relation: "TESTS", confidence}``,
        `source` always a test node, `target` always the covered
        implementation (function/method/class) node — mirroring `calls`'
        caller-to-callee direction convention. Every target here is a
        REAL node already present in `per_file` (no external-stub
        synthesis needed, unlike DM-08 — a test only ever names something
        inside the same extracted project).
    """
    index = _Index(per_file)
    calls_from: dict[str, set[str]] = defaultdict(set)
    for edge in call_edges:
        if edge.get("relation") == "calls":
            calls_from[edge.get("source", "")].add(edge.get("target", ""))
    # Heuristic (4) lookup tables: a target qualifies only when it is
    # defined in production code (non-test file) and is a real code
    # symbol — not a file hub, not a helper defined in a test file.
    node_file: dict[str, str] = {}
    node_kind: dict[str, str] = {}
    for ext in per_file:
        for n in ext.nodes:
            if n.get("id"):
                node_file[n["id"]] = n.get("source_file", "") or ""
                node_kind[n["id"]] = n.get("kind", "") or ""

    edges: list[dict] = []
    seen: set[tuple[str, str]] = set()

    for ext in per_file:
        file_id = _file_hub_id(ext)
        if not file_id:
            continue
        node_by_id = {n["id"]: n for n in ext.nodes if n.get("id")}
        method_class: dict[str, str] = {
            e["target"]: e["source"]
            for e in ext.edges
            if e.get("relation") == "contains" and e.get("target") and e.get("source")
        }

        for node in ext.nodes:
            if node.get("kind") not in (NodeKind.FUNC.value, NodeKind.METHOD.value):
                continue
            label = node.get("label", "")
            test_id = node.get("id", "")
            if not test_id or not label:
                continue
            if not (_is_test_file(node.get("source_file", "")) or _is_test_symbol_name(label)):
                continue

            targets: set[str] = set()
            for impl_name in _impl_name_candidates(label):
                for candidate_id in index.global_funcs.get(impl_name, []):
                    if candidate_id != test_id:
                        targets.add(candidate_id)
            enclosing_class_id = method_class.get(test_id)
            if enclosing_class_id:
                enclosing_label = node_by_id.get(enclosing_class_id, {}).get("label", "")
                for cand in _class_name_candidates(enclosing_label):
                    for classes_in_file in index.classes.values():
                        cid = classes_in_file.get(cand)
                        if cid is not None:
                            targets.add(cid)

            for target_id in targets:
                key = (test_id, target_id)
                if key in seen:
                    continue
                seen.add(key)
                confidence = (
                    Confidence.EXTRACTED
                    if target_id in calls_from.get(test_id, set())
                    else Confidence.INFERRED
                )
                edges.append({
                    "source": test_id, "target": target_id,
                    "relation": "TESTS", "confidence": confidence.value,
                })

            for dotted in _patch_targets(node.get("decorators", "[]")):
                target_id = _resolve_patch_target(index, dotted)
                if target_id is None or target_id == test_id:
                    continue
                key = (test_id, target_id)
                if key in seen:
                    continue
                seen.add(key)
                edges.append({
                    "source": test_id, "target": target_id,
                    "relation": "TESTS", "confidence": Confidence.EXTRACTED.value,
                })

            # Heuristic (4) — direct calls (2026-08-31; see the module
            # docstring for the dogfooding rationale). Runs last so the
            # naming/patch edges (same pair, same EXTRACTED standard)
            # claim the `seen` slot first — no duplicates.
            for target_id in sorted(calls_from.get(test_id, ())):
                if target_id == test_id or (test_id, target_id) in seen:
                    continue
                target_file = node_file.get(target_id, "")
                if not target_file or not _is_production_file(target_file):
                    continue
                if node_kind.get(target_id) not in (
                    NodeKind.FUNC.value, NodeKind.METHOD.value, NodeKind.CLASS.value,
                ):
                    continue
                seen.add((test_id, target_id))
                edges.append({
                    "source": test_id, "target": target_id,
                    "relation": "TESTS", "confidence": Confidence.EXTRACTED.value,
                })

    return edges
