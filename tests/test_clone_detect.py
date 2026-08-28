"""Tests for cie.clone_detect — quality-governance analysis pass
(docs/growth-plan.md Phase 0.5 workstream C: this area had zero test
coverage before this file).

Exercises the token/AST signals (find_token_ast_clones), union-find
fusion (resolve_clone_clusters), and the full analyze() entry point
against real re-parsed source (SourceIndex re-parses from disk, same as
production) over InMemoryRepository — no Neo4j, no embeddings, so the
embedding leg is implicitly exercised as "returns nothing", not skipped.
"""

from __future__ import annotations

from cie import clone_detect, extract
from cie.in_memory_repository import InMemoryRepository
from cie.models import NodeKind


def _write_and_extract(tmp_path, filename: str, source: str):
    path = tmp_path / filename
    path.write_text(source)
    extraction = extract.extract_file(path)
    return extraction


_NEAR_DUPLICATE_A = """
def add_positive(a, b):
    if a > 0 and b > 0:
        return a + b
    return 0
"""

_NEAR_DUPLICATE_B = """
def add_positive_v2(x, y):
    if x > 0 and y > 0:
        return x + y
    return 0
"""

_UNRELATED = """
def greet(name):
    return "hello " + name
"""


def _repo_with(tmp_path, files: dict[str, str]) -> InMemoryRepository:
    all_nodes, all_edges = [], []
    for filename, source in files.items():
        extraction = _write_and_extract(tmp_path, filename, source)
        all_nodes.extend(extraction.nodes)
        all_edges.extend(extraction.edges)
    repo = InMemoryRepository([], [])
    repo.load_extraction(all_nodes, all_edges)
    return repo


def test_find_token_ast_clones_flags_structurally_identical_functions(tmp_path):
    """Token signal should NOT fire (every identifier was renamed) but
    the AST/structural-shape signal should — exactly the case CI-02
    exists to catch that CI-01 misses (see clone_detect.py's own
    docstring)."""
    repo = _repo_with(tmp_path, {"a.py": _NEAR_DUPLICATE_A, "b.py": _NEAR_DUPLICATE_B})
    funcs = [n for n in repo.code_symbol_nodes() if n.kind == NodeKind.FUNC.value]
    assert len(funcs) == 2

    source_index = clone_detect.SourceIndex()
    pairs = clone_detect.find_token_ast_clones(funcs, source_index)
    methods = {method for _a, _b, _score, method in pairs}
    assert "ast" in methods
    assert "token" not in methods  # every identifier differs


def test_find_token_ast_clones_no_match_for_unrelated_functions(tmp_path):
    repo = _repo_with(tmp_path, {"a.py": _NEAR_DUPLICATE_A, "u.py": _UNRELATED})
    funcs = [n for n in repo.code_symbol_nodes() if n.kind == NodeKind.FUNC.value]
    source_index = clone_detect.SourceIndex()
    pairs = clone_detect.find_token_ast_clones(funcs, source_index)
    assert pairs == []


def test_resolve_clone_clusters_fuses_pairs_via_union_find():
    nodes_by_id = {"a": _fake_node("a"), "b": _fake_node("b"), "c": _fake_node("c")}
    # a-b linked directly, b-c linked directly -> one cluster {a, b, c},
    # even though a-c was never compared directly.
    pairs = [("a", "b", 0.9, "ast"), ("b", "c", 0.85, "token")]
    cluster_nodes, edges = clone_detect.resolve_clone_clusters(nodes_by_id, pairs)
    assert len(cluster_nodes) == 1
    assert cluster_nodes[0]["member_count"] == 3
    member_ids = {e["source"] for e in edges}
    assert member_ids == {"a", "b", "c"}
    # consolidation_target is deterministic (highest best-similarity, tie
    # broken by shortest/lexicographic id) — just assert it's one of the members.
    assert cluster_nodes[0]["consolidation_target"] in {"a", "b", "c"}


def test_resolve_clone_clusters_singleton_pairs_produce_no_cluster():
    """A node with no pair at all never forms a cluster (needs >= 2
    members) — resolve_clone_clusters only iterates pair_index, so an
    unpaired node in nodes_by_id is simply absent from the output."""
    nodes_by_id = {"a": _fake_node("a")}
    cluster_nodes, edges = clone_detect.resolve_clone_clusters(nodes_by_id, [])
    assert cluster_nodes == []
    assert edges == []


def test_resolve_clone_clusters_is_idempotent_same_members_same_id():
    nodes_by_id = {"a": _fake_node("a"), "b": _fake_node("b")}
    pairs = [("a", "b", 0.9, "ast")]
    first, _ = clone_detect.resolve_clone_clusters(nodes_by_id, pairs)
    second, _ = clone_detect.resolve_clone_clusters(nodes_by_id, pairs)
    assert first[0]["id"] == second[0]["id"]


def test_analyze_end_to_end_over_in_memory_repository_finds_the_clone(tmp_path):
    repo = _repo_with(tmp_path, {"a.py": _NEAR_DUPLICATE_A, "b.py": _NEAR_DUPLICATE_B})
    summary = clone_detect.analyze(repo)
    assert summary["clusters"] == 1
    assert summary["clustered_symbols"] == 2
    assert summary["clone_coverage_pct"] == 100.0

    # replace_analysis_nodes actually wrote the CloneCluster node — real
    # write path, not just the returned summary dict.
    stored = [n for n in repo._nodes.values() if n.kind == NodeKind.CLONE_CLUSTER.value]
    assert len(stored) == 1


def test_analyze_empty_project_returns_zero_summary_not_an_error(tmp_path):
    repo = InMemoryRepository([], [])
    summary = clone_detect.analyze(repo)
    assert summary == {
        "clusters": 0, "clustered_symbols": 0, "total_symbols": 0,
        "clone_coverage_pct": 0.0,
    }


def _fake_node(node_id: str):
    from cie.models import Node
    return Node(
        id=node_id, label=node_id, source_file="", source_location="",
        file_type="code", kind=NodeKind.FUNC.value, signature="",
        line_start=1, line_end=1, docstring="",
    )
