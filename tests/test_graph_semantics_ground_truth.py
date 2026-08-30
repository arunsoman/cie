"""SEMANTIC ground truth: the answers cie returns must be the exact right
sets, not merely well-formed `ok` envelopes.

Context: the full-surface conformance harness (tool-test-lab/
surface_conformance.py) classifies tools as executable/graceful — but
"ran without crashing" ≠ "returned the truth". This file is the
correctness layer: one indexed-by-construction fixture project where
EVERY true caller/callee/blast-radius member is known by inspection, and
each semantic tool's answer is compared to that ground truth with exact
set equality.

Ground truth (fixture below, line numbers matter):
    app.py:3    alpha()  calls helper()          (app.py:4)
    app.py:6    beta(n)  calls alpha()          (app.py:7)
    mid.py:3    gamma()  calls beta()           (mid.py:4)
    leaf.py:3   delta()  calls gamma()          (leaf.py:4)
    tests/      test_alpha() calls alpha()     (tests line 4)
import edges: app→base, mid→app, leaf→mid, tests→app

Known-by-construction answers this file asserts:
  callers("alpha")      == {beta, test_alpha}          (exact, incl. test caller)
  callers("delta")      == {} — nothing calls leaf functions
  callees("beta")       == {alpha};  callees("delta") == {gamma}
  file_skeleton(app.py) == {alpha, beta} exactly
  blast radius of leaf.py (incoming) == {}
  search_symbol("gamma") finds gamma, nothing else
"""

from __future__ import annotations

from pathlib import Path

import pytest

from cie.factory import build_tool_service_embedded

FILES: dict[str, str] = {
    "base.py": "def helper():\n    return 0\n",
    "app.py": (
        "from base import helper\n"
        "\n"
        "def alpha():\n"
        "    return helper()\n"
        "\n"
        "def beta(n):\n"
        "    return alpha()\n"
    ),
    "mid.py": "from app import beta\n\ndef gamma():\n    return beta(1)\n",
    "leaf.py": "from mid import gamma\n\ndef delta():\n    return gamma()\n",
    "test_app.py": "from app import alpha\n\ndef test_alpha():\n    assert alpha() == 1\n",
}

# hand-derived from the block above — the oracle is the human writing this
# test, never the tool being tested
EXPECT_CALLERS_ALPHA = {"beta", "test_alpha"}
EXPECT_SKELETON_APP = {"alpha", "beta"}
EXPECT_AFFECTED_APP = {"mid.py", "leaf.py", "test_app.py"}


def _sig_name(entry: dict) -> str:
    """Bare symbol name from a hit, tolerating the key spellings the real
    envelopes use: `caller_signature` (callers), `callee_signature`/`callee`
    (callees), `signature` (search_symbol)."""
    for k in ("caller_signature", "signature", "callee_signature"):
        if entry.get(k):
            return entry[k].split("(")[0].strip()
    for k in ("caller", "callee", "name"):
        if entry.get(k):
            return str(entry[k]).strip()
    return ""


@pytest.fixture(scope="module")
def svc(tmp_path_factory):
    root = tmp_path_factory.mktemp("ground_truth")
    for name, text in FILES.items():
        (root / name).write_text(text)
    # index through the exact pipeline `cie index` uses — no shortcuts
    from cie.callgraph import resolve_call_edges
    from cie.embedded_repository import EmbeddedRepository
    from cie.extract import extract_many
    from cie.testlink import resolve_test_edges

    per_file = extract_many(root)
    nodes = [n for ext in per_file for n in ext.nodes]
    call_edges = resolve_call_edges(per_file)
    test_edges = resolve_test_edges(per_file, call_edges)
    edges = [e for ext in per_file for e in ext.edges] + call_edges + test_edges
    EmbeddedRepository(root / ".cie" / "graph.db").load_extraction(nodes, edges)
    return build_tool_service_embedded(root, task_tracking=False)


def _data(env: dict) -> list[dict]:
    assert env.get("ok") is True, f"tool returned error: {json_short(env)}"
    return env.get("results") or env.get("data") or []


def json_short(env: dict) -> str:
    import json
    return json.dumps(env)[:220]


def test_callers_of_alpha_is_exactly_beta_and_test_alpha(svc):
    got = {_sig_name(h) for h in _data(svc.callers("alpha"))}
    assert got == EXPECT_CALLERS_ALPHA, (
        f"blast-radius answer wrong: got {got}, expected {EXPECT_CALLERS_ALPHA}"
    )


def test_callers_of_leaf_is_empty(svc):
    got = {_sig_name(h) for h in _data(svc.callers("delta"))}
    assert got == set(), f"phantom callers of a leaf: {got}"


def test_callees_are_exact(svc):
    assert {_sig_name(h) for h in _data(svc.callees("beta"))} == {"alpha"}
    assert {_sig_name(h) for h in _data(svc.callees("delta"))} == {"gamma"}


def test_file_skeleton_of_app_is_exactly_alpha_beta(svc):
    entries = _data(svc.file_skeleton("app.py"))
    flat: set[str] = set()
    for e in entries:
        for s in e.get("symbols", []):
            flat.add(s["name"])
        if "name" in e and e.get("kind") in ("function", "method"):
            flat.add(e["name"])
    assert flat == EXPECT_SKELETON_APP, f"skeleton wrong: {flat}"


def test_search_symbol_finds_gamma_not_others(svc):
    got = {_sig_name(e) for e in _data(svc.search_symbol("gamma"))}
    assert got == {"gamma"}, f"search_symbol('gamma') leaked: {got}"


def test_no_phantom_hits_for_absent_symbol(svc):
    assert _data(svc.search_symbol("totally_absent_symbol")) == [], (
        "search must not invent hits"
    )


def _affected_files(svc, path: str) -> set[str]:
    """Collect the .py files named in an affected_by envelope, tolerating
    result-shape spellings while still asserting on exact file sets."""
    env = svc.affected_by(path)
    entries = _data(env)
    files: set[str] = set()
    symbols: set[str] = set()
    for e in entries:
        for key in ("file", "path", "source_file", "affected_file"):
            v = str(e.get(key, ""))
            if v.endswith(".py"):
                files.add(Path(v).name)
        if e.get("kind") in ("function", "method") and e.get("name"):
            symbols.add(e["name"])
        if not e.get("file") and e.get("id", "").endswith(".py"):
            files.add(Path(e["id"]).name)
    return files | symbols


def test_blast_radius_of_leaf_is_empty(svc):
    got = _affected_files(svc, "leaf.py")
    assert got == set(), f"nothing depends on leaf.py, tool claimed {got}"


def test_blast_radius_of_app_is_exact(svc):
    """Exact transitive incoming set for app.py: mid.py (imports beta),
    leaf.py (imports gamma from mid), test_app.py (TESTS/call edge: the
    test provably exercises alpha). No less, no more."""
    got = _affected_files(svc, "app.py")
    assert got == EXPECT_AFFECTED_APP, f"blast radius of app.py wrong: {got}"