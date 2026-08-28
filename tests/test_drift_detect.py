"""Tests for cie.drift_detect — quality-governance analysis pass (zero
test coverage before this file, same gap test_clone_detect.py closed for its neighbor
module).

requirement_gap and architecture_drift are pure functions (no LLM, no
Neo4j) — exercised directly, the latter against a real extracted
call-graph cycle so the DFS cycle detection runs over real edges, not a
hand-built fixture pretending to be one.
"""

from __future__ import annotations

from types import SimpleNamespace

from cie import drift_detect, extract
from cie.in_memory_repository import InMemoryRepository


def _task(name: str, file_path: str):
    return SimpleNamespace(name=name, file_path=file_path)


def test_requirement_gap_flags_a_task_whose_file_was_never_indexed():
    tasks = [_task("T1", "src/never_generated.py")]
    findings = drift_detect.requirement_gap(tasks, known_file_paths={"src/other.py"})
    assert len(findings) == 1
    assert findings[0]["drift_type"] == "REQUIREMENT_GAP"
    assert findings[0]["file"] == "src/never_generated.py"


def test_requirement_gap_clears_on_exact_match():
    tasks = [_task("T1", "src/real.py")]
    findings = drift_detect.requirement_gap(tasks, known_file_paths={"src/real.py"})
    assert findings == []


def test_requirement_gap_matches_by_suffix_either_direction():
    """A task's file_path is usually repo-relative
    ('be-v2/src/foo.py'); a FILE node's source_file may be absolute or
    differently-rooted — both suffix directions must resolve (the
    function's own docstring states this; verified here)."""
    tasks = [_task("T1", "src/foo.py")]
    findings = drift_detect.requirement_gap(
        tasks, known_file_paths={"/abs/project/src/foo.py"},
    )
    assert findings == []

    tasks2 = [_task("T2", "/abs/project/src/bar.py")]
    findings2 = drift_detect.requirement_gap(tasks2, known_file_paths={"src/bar.py"})
    assert findings2 == []


def test_requirement_gap_skips_tasks_with_no_file_path():
    tasks = [_task("T1", "")]
    assert drift_detect.requirement_gap(tasks, known_file_paths=set()) == []


def _extract_and_resolve(tmp_path, filenames):
    """Real two-pass load (extraction, then cie.callgraph's cross-file
    call resolution) over `filenames` — same shape `cie index` itself
    uses, not a hand-built edge list pretending to be one."""
    from cie import callgraph

    per_file = [extract.extract_file(tmp_path / f) for f in filenames]
    resolved = callgraph.resolve_call_edges(per_file)
    nodes, edges = [], []
    for e in per_file:
        nodes.extend(e.nodes)
        edges.extend(e.edges)
    repo = InMemoryRepository([], [])
    repo.load_extraction(nodes, edges + resolved)
    return repo


def _repo_with_real_cycle(tmp_path) -> InMemoryRepository:
    """Two files whose functions call each other — a real file-level
    cycle, extracted for real via cie.extract, not hand-built edges."""
    (tmp_path / "a.py").write_text(
        "from b import b_func\n\ndef a_func():\n    return b_func()\n",
    )
    (tmp_path / "b.py").write_text(
        "from a import a_func\n\ndef b_func():\n    return a_func()\n",
    )
    return _extract_and_resolve(tmp_path, ["a.py", "b.py"])


def test_architecture_drift_finds_a_real_circular_file_dependency(tmp_path):
    repo = _repo_with_real_cycle(tmp_path)
    graph_nodes, edge_records = repo.project_graph()
    findings = drift_detect.architecture_drift(graph_nodes, edge_records)
    cycles = [f for f in findings if f["drift_type"] == "CIRCULAR_DEPENDENCY"]
    assert len(cycles) == 1
    assert "a.py" in cycles[0]["detail"] and "b.py" in cycles[0]["detail"]


def test_architecture_drift_no_layer_rules_means_no_layer_findings(tmp_path):
    repo = _repo_with_real_cycle(tmp_path)
    graph_nodes, edge_records = repo.project_graph()
    findings = drift_detect.architecture_drift(graph_nodes, edge_records, layer_rules=None)
    assert all(f["drift_type"] != "LAYER_VIOLATION" for f in findings)


def test_architecture_drift_flags_a_declared_layer_violation(tmp_path):
    (tmp_path / "ui_widget.py").write_text(
        "from db_layer import query\n\ndef render():\n    return query()\n",
    )
    (tmp_path / "db_layer.py").write_text("def query():\n    return []\n")
    repo = _extract_and_resolve(tmp_path, ["ui_widget.py", "db_layer.py"])
    graph_nodes, edge_records = repo.project_graph()

    findings = drift_detect.architecture_drift(
        graph_nodes, edge_records, layer_rules={"ui_": ["db_"]},
    )
    violations = [f for f in findings if f["drift_type"] == "LAYER_VIOLATION"]
    assert len(violations) == 1
    assert "ui_widget.py" in violations[0]["file"]


def test_analyze_end_to_end_writes_findings_and_flags_requirement_gap(tmp_path):
    repo = _repo_with_real_cycle(tmp_path)
    tasks = [_task("T-missing", "src/nonexistent.py")]
    result = drift_detect.analyze(repo, tmp_path, tasks=tasks)
    assert result["by_type"]["REQUIREMENT_GAP"] == 1
    assert result["by_type"]["CIRCULAR_DEPENDENCY"] == 1
    assert result["total"] == 2

    from cie.models import NodeKind
    stored = [n for n in repo._nodes.values() if n.kind == NodeKind.DRIFT_FINDING.value]
    assert len(stored) == result["total"]
