"""CF-08/09: Traceability & Coverage (spec section 14.4).

Pure graph-traversal composition over already-existing edges (`TESTS`
from DM-14, `HAS_CONTRACT` from CF-01) — no new extraction, matching the
spec's own framing ("runs as Graphify Cypher queries against CIE's Neo4j").

Two halves:
- `get_coverage`/`find_orphans`/`traceability_chain` (CODE side): "is this
  code symbol tested? is it contracted?" — operates on the code-structure
  `:Node` graph only.
- `prd_coverage`/`prd_orphans`/`prd_traceability_chain` (PRD-HIERARCHY
  side, added 2026-08-10 as a follow-up): reads `HierarchyNodeView` rows
  from `cie.hierarchy.get_project_tree` (a SEPARATE schema this module
  still doesn't write to or import from directly — the caller fetches
  them and passes plain `HierarchyNodeView`s in, keeping this module's
  own dependency surface unchanged) and cross-references each
  file-bearing hierarchy node (an `AtomicTask`, by this codebase's own
  convention — `node.metadata["file_path"]`) against the code graph's
  `FILE` nodes. This is the half `cie-confidence-framework-implementation.
  md` originally deferred as "a larger, separate integration" — it turned
  out to need no new extraction, just reading two already-existing data
  sources side by side, once one function accepts both.
"""

from __future__ import annotations

from typing import Any, Sequence

from cie.models import EdgeRecord, Node, NodeKind

_CODE_KINDS = (NodeKind.FUNC.value, NodeKind.METHOD.value, NodeKind.CLASS.value)


def get_coverage(nodes: Sequence[Node], edges: Sequence[EdgeRecord]) -> dict:
    """CF-08 `get_coverage()`: what fraction of code symbols (FUNC/
    METHOD/CLASS) have at least one `TESTS` edge, and what fraction have
    at least one `HAS_CONTRACT` edge."""
    code_ids = {n.id for n in nodes if n.kind in _CODE_KINDS}
    tested_ids: set[str] = set()
    contracted_ids: set[str] = set()
    for er in edges:
        e = er.edge
        if e.relation == "TESTS" and e.target in code_ids:
            tested_ids.add(e.target)
        if e.relation == "HAS_CONTRACT" and e.source in code_ids:
            contracted_ids.add(e.source)
    total = len(code_ids)
    return {
        "total_code_symbols": total,
        "tested_count": len(tested_ids),
        "tested_pct": round(len(tested_ids) / total * 100, 1) if total else 0.0,
        "contracted_count": len(contracted_ids),
        "contracted_pct": round(len(contracted_ids) / total * 100, 1) if total else 0.0,
    }


def find_orphans(nodes: Sequence[Node], edges: Sequence[EdgeRecord]) -> list[Node]:
    """CF-08 `find_orphans()`: FUNC/METHOD symbols with NEITHER a `TESTS`
    edge nor a `HAS_CONTRACT` edge — "nothing verifies this exists"."""
    covered: set[str] = set()
    for er in edges:
        e = er.edge
        if e.relation == "TESTS":
            covered.add(e.target)
        if e.relation == "HAS_CONTRACT":
            covered.add(e.source)
    return [
        n for n in nodes
        if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value) and n.id not in covered
    ]


def traceability_chain(nodes: Sequence[Node], edges: Sequence[EdgeRecord], symbol_id: str) -> dict:
    """CF-09 `traceability_chain()`: for one code symbol, its tests
    (`TESTS` edges pointing at it) and contracts (`HAS_CONTRACT` edges
    from it)."""
    tests = [er for er in edges if er.edge.relation == "TESTS" and er.edge.target == symbol_id]
    contracts = [er for er in edges if er.edge.relation == "HAS_CONTRACT" and er.edge.source == symbol_id]
    return {
        "symbol_id": symbol_id,
        "tested_by": [er.edge.source for er in tests],
        "contracts": [er.edge.target for er in contracts],
    }


# -- PRD-hierarchy side (CF-08's other half) ----------------------------------

def _file_path_of(hierarchy_node: Any) -> str:
    """A hierarchy node's declared implementation file, or "" if it
    doesn't carry one (only `AtomicTask`-derived nodes do, per
    `cie.tasks.AtomicTask.file_path`)."""
    return (hierarchy_node.metadata or {}).get("file_path", "")


def prd_coverage(hierarchy_nodes: Sequence[Any], code_nodes: Sequence[Node]) -> dict:
    """CF-08, PRD-hierarchy side: what fraction of file-bearing hierarchy
    nodes (`AtomicTask`s with a declared `file_path`) have a matching
    `FILE` node in the code graph. Matched by substring (a hierarchy
    node's `file_path` is project-root-relative; a `FILE` node's
    `source_file` is typically absolute) rather than exact equality —
    same tolerant-matching approach `quality_report.comprehensiveness_
    report`'s `file_index_pct` already uses for the identical relative-
    vs-absolute-path mismatch.
    """
    file_tasks = [n for n in hierarchy_nodes if _file_path_of(n)]
    code_file_paths = [n.source_file for n in code_nodes if n.kind == NodeKind.FILE.value]
    implemented = [
        n for n in file_tasks
        if any(_file_path_of(n) in p for p in code_file_paths)
    ]
    total = len(file_tasks)
    return {
        "total_prd_file_tasks": total,
        "implemented_count": len(implemented),
        "implemented_pct": round(len(implemented) / total * 100, 1) if total else 0.0,
    }


def prd_orphans(hierarchy_nodes: Sequence[Any], code_nodes: Sequence[Node]) -> list[dict]:
    """CF-08, PRD-hierarchy side: file-bearing hierarchy nodes with NO
    matching `FILE` node in the code graph — "the spec says this file
    should exist; the code graph has nothing there." The inverse
    direction (code with no PRD link) is `find_orphans` above."""
    code_file_paths = [n.source_file for n in code_nodes if n.kind == NodeKind.FILE.value]
    out: list[dict] = []
    for n in hierarchy_nodes:
        file_path = _file_path_of(n)
        if not file_path:
            continue
        if not any(file_path in p for p in code_file_paths):
            out.append({"hierarchy_node_id": n.id, "name": n.name, "file_path": file_path})
    return out


def prd_traceability_chain(
    hierarchy_nodes: Sequence[Any], code_nodes: Sequence[Node], edges: Sequence[EdgeRecord], task_id: str,
) -> dict:
    """CF-09, PRD-hierarchy side: the full chain the spec's own
    `traceability_chain(task_id)` describes — task -> implemented_in
    (matching `FILE` node(s)) -> tested_by (`TESTS` edges on symbols in
    that file). `derived_from` (the task's PRD lineage) is deliberately
    NOT included — that's `cie.hierarchy.get_lineage(task_id)`'s own,
    already-built job; duplicating it here would create two ways to
    answer the same question. `found: False` when `task_id` isn't in
    `hierarchy_nodes` at all, distinct from `found: True` with an empty
    `implemented_in` (task exists but nothing implements it yet).
    """
    task = next((n for n in hierarchy_nodes if n.id == task_id), None)
    if task is None:
        return {"task_id": task_id, "found": False}
    file_path = _file_path_of(task)
    implemented_in = sorted({
        n.source_file for n in code_nodes
        if n.kind == NodeKind.FILE.value and file_path and file_path in n.source_file
    })
    code_ids_in_file = {n.id for n in code_nodes if n.source_file in implemented_in}
    tested_by = sorted({
        er.edge.source for er in edges
        if er.edge.relation == "TESTS" and er.edge.target in code_ids_in_file
    })
    return {
        "task_id": task_id, "found": True, "file_path": file_path,
        "implemented_in": implemented_in, "tested_by": tested_by,
    }
