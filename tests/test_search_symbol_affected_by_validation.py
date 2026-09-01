"""Regression tests for the `kind`/`direction` validation gaps found and
fixed 2026-09-01, matching the same fix already landed in atomic-forge's
tools.py (a downstream consumer of this exact package): an unrecognized
`kind` used to return a generic "no symbol found" hint indistinguishable
from a genuinely-absent name, and an unrecognized `direction` silently
fell through to "outgoing" with no signal the input wasn't understood.

Covers both places the fix landed:
  - `cie.tools.ToolService` (cie/tools/__init__.py) — the shared core
    both the MCP and native-Python channels run through.
  - `cie.tools.heuristic.HeuristicToolSet` — has its own external callers
    (forge/tools.py's LocalDiskBackend), guarded independently.
"""
from __future__ import annotations

from pathlib import Path

from cie.factory import build_tool_service_embedded
from cie.tools.heuristic import HeuristicToolSet
from cie.tools.index import SymbolIndex


def _svc(tmp_path: Path, files: dict[str, str]):
    for name, text in files.items():
        (tmp_path / name).write_text(text)
    return build_tool_service_embedded(tmp_path, task_tracking=False)


# ---------------------------------------------------------- ToolService ----

def test_search_symbol_invalid_kind_gets_specific_hint(tmp_path):
    svc = _svc(tmp_path, {"a.py": "def helper():\n    return 1\n"})
    r = svc.search_symbol("helper", kind="import")
    assert r["ok"] is True
    assert r["results"] == []
    assert "not a value this graph has ever produced" in r["hint"]
    # a genuinely valid kind is unaffected
    ok = svc.search_symbol("helper", kind="function")
    assert ok["results"]


def test_search_symbol_kind_method_finds_a_real_method(tmp_path):
    """extract.py tags a function defined inside a class NodeKind.METHOD
    ("method"), not FUNC — a real, populated kind, same as the Java/C++
    case that regressed downstream in atomic-forge."""
    svc = _svc(tmp_path, {
        "a.py": "class Foo:\n    def bar(self, x):\n        return x + 1\n",
    })
    r = svc.search_symbol("bar", kind="method")
    assert r["results"], r
    assert r["results"][0]["kind"] == "method"


def test_affected_by_invalid_direction_is_not_silently_wrong(tmp_path):
    svc = _svc(tmp_path, {
        "a.py": "def helper():\n    return 1\n",
        "b.py": "from a import helper\ndef main():\n    return helper()\n",
    })
    incoming = svc.affected_by("a.py", direction="incoming")
    assert incoming["results"]  # b.py depends on a.py

    for bad in ("INCOMING", "both", "reverse"):
        r = svc.affected_by("a.py", direction=bad)
        assert r["results"] == [], f"direction={bad!r} should not silently answer: {r}"
        assert "not a recognized value" in r["hint"]


# ---------------------------------------------------------- HeuristicToolSet ----

def _heuristic(tmp_path: Path, files: dict[str, str]) -> HeuristicToolSet:
    for name, text in files.items():
        (tmp_path / name).write_text(text)
    index = SymbolIndex(tmp_path)
    index.build()
    return HeuristicToolSet(tmp_path, index)


def test_heuristic_search_symbol_invalid_kind_gets_hint(tmp_path):
    """Guarded independently of ToolService — this class has its own
    external callers (see its module docstring)."""
    tools = _heuristic(tmp_path, {"a.py": "def helper():\n    return 1\n"})
    r = tools.search_symbol("helper", kind="import")
    assert r["results"] == []
    assert "not a recognized filter" in r["hint"]
    ok = tools.search_symbol("helper", kind="function")
    assert ok["results"]


def test_heuristic_affected_by_invalid_direction_is_not_silently_wrong(tmp_path):
    tools = _heuristic(tmp_path, {
        "a.py": "def helper():\n    return 1\n",
        "b.py": "from a import helper\ndef main():\n    return helper()\n",
    })
    for bad in ("INCOMING", "both"):
        r = tools.affected_by("a.py", direction=bad)
        assert r["results"] == [], f"direction={bad!r} should not silently answer: {r}"
        assert "not a recognized value" in r["hint"]
