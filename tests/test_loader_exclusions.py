"""`cie index .` must never index dependency/cache/VCS/build directories.

Found by dogfooding (2026-08-30): `cie index .` on THIS repo walked into
`.venv/` — which holds a stale `pip install` of cie itself — and indexed
1,779 files / ~28k nodes, mostly site-packages. Every blast-radius answer
came back corrupted: `callers("extract_many")` resolved into
`.venv/lib/python3.13/site-packages/cie/extract.py`, so each real call
site appeared twice (live tree + stale copy) plus pytest-internal noise.

The fix lives in the SINGLE walk behind `cie index`, `cie reindex` and
`graph_diff` (`cie.extract.extract_many` / `extract_tree`): directories in
`EXCLUDED_DIRS` are never entered. These tests pin that invariant.
"""

from __future__ import annotations

import json
from pathlib import Path

from cie.extract import EXCLUDED_DIRS, extract_many, extract_tree
from cie.callgraph import resolve_call_edges


def _make_tree(tmp_path: Path) -> Path:
    """A plausible project: two real source files next to every junk-dir
    class the walk used to swallow, including a stale copy of the project
    inside `.venv` (the exact shape that corrupted the live graph)."""
    root = tmp_path / "proj"
    pkg = root / "pkg"
    pkg.mkdir(parents=True)
    (root / "app.py").write_text("def alpha():\n    return 1\n")
    (pkg / "mod.py").write_text(
        "from app import alpha\n\n\ndef beta():\n    return alpha()\n"
    )

    venv_sp = root / ".venv" / "lib" / "python3.99" / "site-packages" / "stale_copy"
    venv_sp.mkdir(parents=True)
    (venv_sp / "app.py").write_text("def alpha():\n    return 999\n")

    (root / "venv" / "lib").mkdir(parents=True)
    (root / "venv" / "lib" / "old.py").write_text("def only_in_venv():\n    return 2\n")
    (root / "__pycache__").mkdir()
    (root / "__pycache__" / "cache.py").write_text("def only_in_pycache():\n    return 3\n")
    (root / "node_modules" / "dep").mkdir(parents=True)
    (root / "node_modules" / "dep" / "index.py").write_text(
        "def only_in_node_modules():\n    return 4\n"
    )
    (root / ".git" / "hooks").mkdir(parents=True)
    (root / ".git" / "hooks" / "hook.py").write_text("def only_in_git():\n    return 5\n")
    (root / "build" / "lib").mkdir(parents=True)
    (root / "build" / "lib" / "stale.py").write_text(
        "def only_in_build():\n    return 6\n"
    )
    (root / ".mypy_cache" / "x").mkdir(parents=True)
    (root / ".mypy_cache" / "x" / "m.py").write_text(
        "def only_in_mypy():\n    return 7\n"
    )
    return root


def _all_names(root: Path, extractions) -> set[str]:
    return {n["label"] for ext in extractions for n in ext.nodes if "label" in n}


def _assert_no_junk_nodes(root: Path, extractions) -> None:
    for ext in extractions:
        for n in ext.nodes:
            rel = Path(n["id"]).relative_to(root).parts
            assert not (set(rel) & EXCLUDED_DIRS), f"junk node indexed: {n['id']}"


def test_extract_many_skips_dependency_cache_vcs_dirs(tmp_path):
    root = _make_tree(tmp_path)
    extractions = extract_many(root)
    assert extractions, "the real project sources must still be extracted"

    names = _all_names(root, extractions)
    assert "alpha" in names and "beta" in names
    for marker in ("only_in_venv", "only_in_pycache", "only_in_node_modules",
                   "only_in_git", "only_in_build", "only_in_mypy"):
        assert marker not in names, f"junk symbol `{marker}` made it into the graph"


def test_extract_many_never_index_the_stale_venv_copy(tmp_path):
    root = _make_tree(tmp_path)
    extractions = extract_many(root)

    file_ids = {n["id"] for ext in extractions for n in ext.nodes
                if n.get("kind") == "file"}
    # the stale copy under .venv must not exist in the graph, project files must
    assert not [f for f in file_ids if ".venv" in f]
    assert {f for f in file_ids if f.endswith("app.py")
            or f.endswith(os_join("pkg", "mod.py"))} == file_ids


def test_no_duplicate_alpha_definition_from_the_venv_copy(tmp_path):
    """The live bug in one assert: a stale pip-installed copy of the project
    inside .venv must not contribute a second definition of the same symbol
    (that is what duplicated every callers() answer)."""
    root = _make_tree(tmp_path)
    per_file = extract_many(root)
    alpha_defs = [n for ext in per_file for n in ext.nodes
                  if n.get("label") == "alpha" and "called_name" not in n]
    assert len(alpha_defs) == 1, \
        f"symbol defined {len(alpha_defs)}x in the graph: {[d['id'] for d in alpha_defs]}"
    call_edges = resolve_call_edges(per_file)
    assert json.dumps(call_edges).count("alpha") >= 1


def test_extract_tree_merges_only_project_sources(tmp_path):
    root = _make_tree(tmp_path)
    merged = extract_tree(root)
    names = {n["label"] for n in merged.nodes if "label" in n}
    assert {"alpha", "beta"} <= names
    assert not (names & {
        "only_in_venv", "only_in_pycache", "only_in_node_modules",
        "only_in_git", "only_in_build", "only_in_mypy",
    })


def os_join(*parts: str) -> str:
    return str(Path(*parts))