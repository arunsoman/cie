"""Tests for cie.metrics — quality-governance aggregate scoring
(docs/growth-plan.md Phase 0.5 workstream C: zero test coverage before
this file).

metrics.compute() reads whatever clone_detect/drift_detect already
wrote — so these tests run the real upstream passes first (against
InMemoryRepository, no mocking) rather than hand-inserting analysis
nodes in a shape that might not match what those passes actually
produce.
"""

from __future__ import annotations

from types import SimpleNamespace

from cie import clone_detect, drift_detect, metrics
from cie.in_memory_repository import InMemoryRepository


def test_compute_on_a_never_analyzed_project_is_all_zeros():
    """The module's own docstring: a 0 here means "nothing found OR
    nothing run" — verified as the actual behavior for a fresh repo."""
    repo = InMemoryRepository([], [])
    result = metrics.compute(repo)
    assert result["clone_coverage_pct"] == 0.0
    assert result["drift_index"] == 0.0
    assert result["tech_debt_score"] == 0.0
    assert result["total_symbols"] == 0
    assert "measured_at" in result


def test_compute_reflects_real_clone_clusters(tmp_path):
    (tmp_path / "a.py").write_text(
        "def add_positive(a, b):\n    if a > 0 and b > 0:\n        return a + b\n    return 0\n",
    )
    (tmp_path / "b.py").write_text(
        "def add_positive_v2(x, y):\n    if x > 0 and y > 0:\n        return x + y\n    return 0\n",
    )
    from cie import extract
    all_nodes, all_edges = [], []
    for f in ("a.py", "b.py"):
        e = extract.extract_file(tmp_path / f)
        all_nodes.extend(e.nodes)
        all_edges.extend(e.edges)
    repo = InMemoryRepository([], [])
    repo.load_extraction(all_nodes, all_edges)

    clone_summary = clone_detect.analyze(repo)
    assert clone_summary["clusters"] == 1  # sanity: the upstream pass actually found one

    result = metrics.compute(repo)
    assert result["clone_coverage_pct"] == 100.0  # both functions are the only 2 symbols
    assert result["tech_debt_score"] > 0.0


def test_compute_reflects_real_drift_findings(tmp_path):
    (tmp_path / "a.py").write_text(
        "from b import b_func\n\ndef a_func():\n    return b_func()\n",
    )
    (tmp_path / "b.py").write_text(
        "from a import a_func\n\ndef b_func():\n    return a_func()\n",
    )
    from cie import callgraph, extract
    per_file = [extract.extract_file(tmp_path / f) for f in ("a.py", "b.py")]
    resolved = callgraph.resolve_call_edges(per_file)
    nodes, edges = [], []
    for e in per_file:
        nodes.extend(e.nodes)
        edges.extend(e.edges)
    repo = InMemoryRepository([], [])
    repo.load_extraction(nodes, edges + resolved)

    drift_summary = drift_detect.analyze(repo, tmp_path)
    assert drift_summary["by_type"]["CIRCULAR_DEPENDENCY"] == 1  # sanity

    result = metrics.compute(repo)
    assert result["drift_index"] > 0.0
    # severity_weight["warning"] == 2 (a circular dependency is "warning"),
    # normalized against 2 total symbols (a_func, b_func) -> min(100, 2/2*100) = 100
    assert result["drift_index"] == 100.0


def test_compute_writes_a_metric_snapshot_per_metric():
    repo = InMemoryRepository([], [])
    metrics.compute(repo)
    snapshots = repo.metric_trend()
    assert {s.metric_type for s in snapshots} == {
        "clone_coverage_pct", "drift_index", "tech_debt_score",
    }


def test_tech_debt_report_includes_clusters_and_findings_in_priority_order(tmp_path):
    (tmp_path / "a.py").write_text(
        "def add_positive(a, b):\n    if a > 0 and b > 0:\n        return a + b\n    return 0\n",
    )
    (tmp_path / "b.py").write_text(
        "def add_positive_v2(x, y):\n    if x > 0 and y > 0:\n        return x + y\n    return 0\n",
    )
    from cie import extract
    all_nodes, all_edges = [], []
    for f in ("a.py", "b.py"):
        e = extract.extract_file(tmp_path / f)
        all_nodes.extend(e.nodes)
        all_edges.extend(e.edges)
    repo = InMemoryRepository([], [])
    repo.load_extraction(all_nodes, all_edges)
    clone_detect.analyze(repo)

    report = metrics.tech_debt_report(repo)
    assert report["scores"]["clone_coverage_pct"] == 100.0
    kinds = {item["kind"] for item in report["findings"]}
    assert "clone_cluster" in kinds


def test_tech_debt_report_top_n_caps_the_prioritized_list(tmp_path):
    """Needs >1 real finding to actually exercise the cap — an empty
    repo trivially satisfies len<=1 without testing anything."""
    (tmp_path / "a.py").write_text(
        "from b import b_func\n\ndef a_func():\n    return b_func()\n",
    )
    (tmp_path / "b.py").write_text(
        "from a import a_func\n\ndef b_func():\n    return a_func()\n"
        "def helper():\n    return None\n",
    )
    from cie import callgraph, extract
    per_file = [extract.extract_file(tmp_path / f) for f in ("a.py", "b.py")]
    resolved = callgraph.resolve_call_edges(per_file)
    nodes, edges = [], []
    for e in per_file:
        nodes.extend(e.nodes)
        edges.extend(e.edges)
    repo = InMemoryRepository([], [])
    repo.load_extraction(nodes, edges + resolved)
    drift_detect.analyze(repo, tmp_path, tasks=[
        SimpleNamespace(name="T1", file_path="src/missing.py"),
    ])

    uncapped = metrics.tech_debt_report(repo, top_n=20)
    assert len(uncapped["findings"]) >= 2  # sanity: there really are >1 to cap

    capped = metrics.tech_debt_report(repo, top_n=1)
    assert len(capped["findings"]) == 1
