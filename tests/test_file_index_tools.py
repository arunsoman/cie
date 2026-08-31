"""The file-path index (cie.file_index) and its navigation tools:
ls / dir / file_hierarchy / file_names_like / path_prefix.

Contract under test — same single-source discipline as the AST mirror:

1. every navigation read serves ONLY the in-process index (one lazy
   os.walk per root per process, pruning cie.extract.EXCLUDED_DIRS);
2. every cie write path updates the index in the same call as the
   filesystem write — a file cie just wrote is listable immediately, a
   deleted file is unlistable immediately;
3. external changes stay invisible until reindex()/reindex_file
   explicitly refreshes — proven here by writing and deleting a file
   behind cie's back;
4. every envelope is bounded and honest: real counts + truncated flags
   + hints, never a silent cut.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from cie.embedded_repository import NullTaskRepository
from cie.in_memory_repository import InMemoryRepository
from cie.query import QueryEngine
from cie.tools import ToolService


@pytest.fixture()
def project_root(tmp_path: Path) -> Path:
    (tmp_path / "src" / "sub").mkdir(parents=True)
    (tmp_path / "tests").mkdir()
    (tmp_path / "src" / "payment.py").write_text("def charge() -> int:\n    return 1\n")
    (tmp_path / "src" / "sub" / "deep.py").write_text("def deep() -> int:\n    return 2\n")
    (tmp_path / "tests" / "test_payment.py").write_text("def test_charge():\n    assert True\n")
    (tmp_path / "README.md").write_text("# readme\n")
    # excluded trees — must never enter the index
    (tmp_path / ".git").mkdir()
    (tmp_path / ".git" / "config.py").write_text("x = 1\n")
    (tmp_path / "__pycache__").mkdir()
    (tmp_path / "__pycache__" / "payment.cpython-313.pyc").write_bytes(b"\x00")
    (tmp_path / "node_modules").mkdir()
    (tmp_path / "node_modules" / "lib.js").write_text("module.exports = 1\n")
    return tmp_path


@pytest.fixture()
def service(project_root: Path) -> ToolService:
    return ToolService(
        QueryEngine(InMemoryRepository([], [])),
        NullTaskRepository(),
        root=project_root,
    )


# ---------------------------------------------------------------------------


def test_ls_lists_one_level_from_the_index(service):
    env = service.ls("")
    assert env["ok"], env
    row = env["results"][0]
    assert [d["name"] for d in row["dirs"]] == ["src", "tests"]
    assert [d["files_within"] for d in row["dirs"]] == [2, 1]
    assert row["files"] == [{"name": "README.md", "file_type": "markdown"}]

    inner = service.ls("src")["results"][0]
    assert [d["name"] for d in inner["dirs"]] == ["sub"]
    assert [f["name"] for f in inner["files"]] == ["payment.py"]


def test_excluded_dirs_never_enter_the_index(service):
    row = service.ls("")["results"][0]
    names = {d["name"] for d in row["dirs"]} | {f["name"] for f in row["files"]}
    assert names == {"src", "tests", "README.md"}  # no .git/__pycache__/node_modules
    everything = service.file_names_like("*")["results"][0]
    assert everything["count"] == 4


def test_ls_of_a_file_redirects_to_content_tools(service):
    env = service.ls("src/payment.py")
    assert env["ok"] is False
    assert env["error"]["kind"] == "validation"
    assert "get_meta" in env["hint"]


def test_dir_is_the_ls_alias_with_its_own_tool_name(service):
    ls = service.ls("src")
    d = service.dir("src")
    assert d["ok"] and d["tool"] == "dir"
    assert d["results"][0] == ls["results"][0]


def test_file_hierarchy_renders_the_tree(service):
    env = service.file_hierarchy("", depth=2)
    assert env["ok"], env
    row = env["results"][0]
    assert "src/ (2 files)" in row["tree"]
    assert "payment.py" in row["tree"] and "sub/ (1 files)" in row["tree"]
    assert "README.md" in row["tree"]
    assert row["files_within"] == 4 and row["dirs_within"] == 3
    assert env["truncated"] is False

    tight = service.file_hierarchy("", depth=2, max_entries=2)
    assert tight["truncated"] is True
    assert "budget reached" in tight["hint"]


def test_file_names_like_glob_search(service):
    env = service.file_names_like("*payment*")
    row = env["results"][0]
    assert row["matches"] == ["src/payment.py", "tests/test_payment.py"]
    assert row["count"] == 2 and env["truncated"] is False

    miss = service.file_names_like("*nope*")
    assert miss["results"][0]["count"] == 0
    assert "fnmatch" in miss["hint"]

    capped = service.file_names_like("*", limit=2)
    assert capped["truncated"] is True and capped["results"][0]["count"] == 4


def test_path_prefix_recovers_partial_and_mistyped_paths(service):
    env = service.path_prefix("sr")
    row = env["results"][0]
    assert row["file_count"] == 2
    assert row["dirs"] == ["src", "src/sub"]      # ancestors of the matches

    exact = service.path_prefix("src/sub")
    assert exact["results"][0]["files"] == ["src/sub/deep.py"]

    typo = service.path_prefix("srx")
    row = typo["results"][0]
    assert row["file_count"] == 0
    assert "nearest indexed paths" in typo["hint"]
    # the two sort-order-adjacent neighbors around the insertion point for
    # "srx" — not every file under src/, just the bisect's immediate pair
    assert "src/sub/deep.py" in typo["hint"]
    assert "tests/test_payment.py" in typo["hint"]


def test_cie_writes_update_the_index_in_the_same_call(service):
    service.write_file("src/new_file.py", "def fresh() -> int:\n    return 3\n")
    inner = service.ls("src")["results"][0]
    assert "new_file.py" in [f["name"] for f in inner["files"]]

    service.delete_file("src/new_file.py")
    inner = service.ls("src")["results"][0]
    assert "new_file.py" not in [f["name"] for f in inner["files"]]


def test_external_changes_invisible_until_reindex(service, project_root):
    # force the index's first (lazy) build BEFORE the external change lands,
    # so "invisible until reindex" is actually exercised — otherwise the
    # external write would be sitting on disk before the index ever walks it
    service.ls("")
    # an external add
    (project_root / "external.py").write_text("x = 1\n")
    assert service.file_names_like("external*")["results"][0]["count"] == 0
    service.reindex()
    assert service.file_names_like("external*")["results"][0]["count"] == 1

    # an external delete of a previously-indexed file
    (project_root / "README.md").unlink()
    assert "README.md" in service.file_names_like("README*")["results"][0]["matches"]
    service.reindex()
    assert service.file_names_like("README*")["results"][0]["count"] == 0


def test_jail_still_holds_for_navigation(service):
    escape = service.ls("../")
    assert escape["ok"] is False
    assert "escapes root" in escape["error"]["message"]


def test_empty_dir_is_invisible_and_the_envelope_says_why(service, project_root):
    (project_root / "emptydir").mkdir()
    env = service.ls("emptydir")
    assert env["ok"], env
    row = env["results"][0]
    assert row["dirs"] == [] and row["files"] == []
    assert "no files under this path" in env["hint"]


def test_navigation_tools_exposed_on_all_surfaces():
    """HTTP + policy + schema + describe — the surface-invariants suite
    pins the general rule; this pins the five names specifically."""
    from cie.routes import READ_ONLY_CIE_TOOLS, TOOLS
    from cie.tool_policy import WRITE_TOOLS
    from cie.tool_schema import tool_schemas
    from cie.tools import ToolService

    names = {"ls", "dir", "file_hierarchy", "file_names_like", "path_prefix"}
    assert names <= set(TOOLS)
    assert names <= READ_ONLY_CIE_TOOLS
    assert not (names & WRITE_TOOLS)
    assert names <= {t["name"] for t in tool_schemas(ToolService)}
    assert names <= {name for name in vars(ToolService)
                     if callable(getattr(ToolService, name, None))}