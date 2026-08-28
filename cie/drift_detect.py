"""CI-10/11/12: non-semantic Drift Detector.

Three checks, each comparing a different pair of "what was intended" vs
"what actually is":

- **Requirement gap** (CI-10): AtomicTask.file_path (what the task store
  says should exist) vs the code graph's FILE nodes (what actually got
  indexed). One direction only — "task claims a file that isn't in the
  graph" — not the reverse ("orphan" code with no task): most internal
  helper functions never map 1:1 to a task, so that direction is mostly
  noise, not a real drift signal. Cut for scope, not forgotten.
- **API contract drift** (CI-11): reuses `cie.api_routes`' EXISTING
  filesystem scan (`extract_backend_routes`/`extract_frontend_calls`,
  the same regex-based extraction `resolve_api_route`/`api_call_sites`
  already do per-lookup) but runs it as a full project-wide diff instead
  of a single-path lookup, and persists the result as graph findings
  instead of only ever answering one query at a time.
- **Architecture drift** (CI-12): circular file-level dependencies,
  derived from the code graph's OWN `calls` edges (no new extraction
  needed — a file-to-file dependency exists whenever a symbol in file A
  calls a symbol in file B). Layer-boundary rules are deliberately NOT
  hardcoded here (per spec section 13.3's "configurable" framing) — a
  caller wanting that passes its own `layer_rules`.

Explicitly cut for scope (see
be-v2/docs/cie-code-intelligence-implementation.md): CI-13 (semantic
drift — the spec itself flags this as high-false-positive without
tuning) and CI-14 (dependency drift — needs manifest parsing, lower
value than the three above).
"""
from __future__ import annotations

import hashlib
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional, Sequence

from cie import api_routes
from cie.models import Confidence, EdgeRecord, Node, NodeKind
from cie.repository import Repository


def requirement_gap(tasks: Sequence[Any], known_file_paths: set[str]) -> list[dict]:
    """CI-10: tasks whose declared `file_path` matches no FILE node's
    `source_file` in the code graph. `known_file_paths` is exact-or-
    suffix matched (a task's path is usually repo-relative, e.g.
    `be-v2/src/features/foo/bar.py`; a FILE node's `source_file` may be
    absolute or relative depending on how it was loaded) — both
    directions of suffix containment are checked so either convention
    resolves."""
    findings: list[dict] = []
    for task in tasks:
        file_path = (getattr(task, "file_path", "") or "").strip()
        if not file_path:
            continue
        matched = any(
            file_path == known or known.endswith("/" + file_path)
            or file_path.endswith("/" + known)
            for known in known_file_paths
        )
        if matched:
            continue
        findings.append({
            "drift_type": "REQUIREMENT_GAP",
            "severity": "warning",
            "file": file_path,
            "detail": f"task '{getattr(task, 'name', '')}' specifies "
                      f"{file_path}, not found in the code graph — the "
                      "file was never generated, or was generated "
                      "somewhere the graph hasn't indexed",
        })
    return findings


def api_contract_drift(repo_root: Path) -> list[dict]:
    """CI-11: full project-wide diff between backend routes and frontend
    call sites (reuses `cie.api_routes`' existing scan, run once over
    every route/call instead of `resolve_api_route`/`api_call_sites`'s
    one-lookup-at-a-time shape)."""
    routes = api_routes.extract_backend_routes(repo_root)
    calls = api_routes.extract_frontend_calls(repo_root)
    findings: list[dict] = []
    for route in routes:
        if any(api_routes.paths_match(route.normalized_path, c.normalized_path) for c in calls):
            continue
        findings.append({
            "drift_type": "UNUSED_ROUTE",
            "severity": "info",
            "file": route.file,
            "line": route.line,
            "detail": f"{route.method} {route.path} ({route.file}:{route.line}) "
                      "has no matching frontend caller",
        })
    for call in calls:
        if any(api_routes.paths_match(call.normalized_path, r.normalized_path) for r in routes):
            continue
        findings.append({
            "drift_type": "MISSING_ENDPOINT",
            "severity": "critical",
            "file": call.file,
            "line": call.line,
            "detail": f"frontend calls {call.path} ({call.file}:{call.line}) "
                      "with no matching backend route",
        })
    return findings


def architecture_drift(
    nodes: Sequence[Node], edges: Sequence[EdgeRecord],
    layer_rules: Optional[dict[str, list[str]]] = None,
) -> list[dict]:
    """CI-12: circular file-level dependencies derived from `calls`
    edges, plus optional layer-boundary violations.

    `layer_rules` (optional): `{"layer_name": ["forbidden_substring", ...]}`
    — a file whose path contains `layer_name` importing (calling into) a
    file whose path contains one of the forbidden substrings is flagged.
    Empty/`None` (the default) means "no layer rules configured" — this
    function then reports ONLY circular dependencies, never invents
    layer boundaries this project hasn't declared.
    """
    by_id = {n.id: n for n in nodes}
    file_calls: set[tuple[str, str]] = set()
    for er in edges:
        if er.edge.relation != "calls":
            continue
        src = by_id.get(er.edge.source)
        tgt = by_id.get(er.edge.target)
        if src is None or tgt is None:
            continue
        if src.source_file and tgt.source_file and src.source_file != tgt.source_file:
            file_calls.add((src.source_file, tgt.source_file))

    adj: dict[str, set[str]] = defaultdict(set)
    for a, b in file_calls:
        adj[a].add(b)

    findings: list[dict] = []
    seen_cycles: set[frozenset] = set()
    # White/gray/black DFS cycle detection — reports the first cycle
    # found through each starting file, deduplicated by the SET of files
    # involved (so A->B->A and B->A->B, the same cycle from either
    # entry point, are reported once).
    color: dict[str, int] = {}  # 0=white(default) 1=gray 2=black

    def _dfs(node: str, stack: list[str]) -> None:
        color[node] = 1
        stack.append(node)
        for neighbor in adj.get(node, ()):
            if color.get(neighbor, 0) == 1:
                cycle_start = stack.index(neighbor)
                cycle = stack[cycle_start:] + [neighbor]
                key = frozenset(cycle)
                if key not in seen_cycles:
                    seen_cycles.add(key)
                    findings.append({
                        "drift_type": "CIRCULAR_DEPENDENCY",
                        "severity": "warning",
                        "file": cycle[0],
                        "detail": "circular file dependency: " + " -> ".join(cycle),
                    })
            elif color.get(neighbor, 0) == 0:
                _dfs(neighbor, stack)
        stack.pop()
        color[node] = 2

    for file in list(adj.keys()):
        if color.get(file, 0) == 0:
            _dfs(file, [])

    if layer_rules:
        for a, b in file_calls:
            for layer, forbidden in layer_rules.items():
                if layer not in a:
                    continue
                if any(f in b for f in forbidden):
                    findings.append({
                        "drift_type": "LAYER_VIOLATION",
                        "severity": "warning",
                        "file": a,
                        "detail": f"{a} (layer '{layer}') calls into {b}, "
                                  f"which matches a forbidden dependency for that layer",
                    })

    return findings


def analyze(
    repo: Repository, repo_root: Path, tasks: Sequence[Any] = (),
    project: str = "", layer_rules: Optional[dict[str, list[str]]] = None,
) -> dict:
    """Run all three drift checks and write `DriftFinding` nodes via
    `replace_analysis_nodes` — the entry point `ToolService.
    drift_detect_run` calls. `tasks` (AtomicTask-shaped objects with
    `.file_path`/`.name`) is supplied by the caller (`ToolService`
    already holds both the query engine and the task repository; this
    module stays decoupled from `cie.task_repository`'s exact interface
    by taking already-fetched tasks as plain data)."""
    files = repo.list_files() if hasattr(repo, "list_files") else []
    known_paths = {f.source_file for f in files if f.source_file}
    nodes, edge_records = repo.project_graph()

    findings: list[dict] = []
    findings.extend(requirement_gap(tasks, known_paths))
    findings.extend(api_contract_drift(repo_root))
    findings.extend(architecture_drift(nodes, edge_records, layer_rules=layer_rules))

    finding_nodes: list[dict] = []
    for f in findings:
        key = f"{f['drift_type']}|{f.get('file', '')}|{f['detail']}"
        finding_id = "driftfinding::" + hashlib.sha1(key.encode("utf-8")).hexdigest()[:16]
        finding_nodes.append({
            "id": finding_id,
            "label": f"{f['drift_type']}: {f.get('file', '')}",
            "source_file": f.get("file", ""),
            "source_location": f"L{f['line']}" if f.get("line") else "",
            "file_type": "analysis",
            "kind": NodeKind.DRIFT_FINDING.value,
            "signature": "",
            "line_start": f.get("line", 0),
            "line_end": f.get("line", 0),
            "docstring": "",
            "drift_type": f["drift_type"],
            "severity": f["severity"],
            "detail": f["detail"],
        })

    repo.replace_analysis_nodes(NodeKind.DRIFT_FINDING.value, finding_nodes, [], project=project)

    by_type: dict[str, int] = defaultdict(int)
    for f in findings:
        by_type[f["drift_type"]] += 1
    return {"total": len(findings), "by_type": dict(by_type)}
