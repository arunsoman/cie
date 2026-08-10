"""CI-06/07/08: static Performance Analyzer.

Two sub-analyzers, both re-parsing source via `cie.source_analysis`
(same "re-locate the tree-sitter node for a graph FUNC/METHOD node"
bridge `cie.clone_detect` uses — see that module's docstring for why
this is a second parse, and why that's the right tradeoff for an
explicit, on-demand analysis pass):

- **Big-O estimation** (CI-06): loop nesting depth + a crude recursion
  check, combined into a `complexity_class` label written directly onto
  each FUNC/METHOD node (`Neo4jRepository.update_node_properties` — a
  patch, not a replace, so nothing else about the node changes).
- **Anti-pattern detection** (CI-07): pattern-matches loop bodies for
  N+1 query shapes, nested loops, sync I/O in a loop, and unbounded
  recursion. Findings are written as `AntiPattern` nodes (same
  `replace_analysis_nodes` write path `CloneCluster` uses), linked via
  `HAS_ANTIPATTERN` edges.

**CI-08** (hot-path flagging) needs no new logic at all: it's the SAME
question `Neo4jRepository.god_nodes` already answers (most-connected
real entities, file hubs excluded) — this module just writes that
answer onto the FUNC/METHOD nodes it names as `hot_path: true`, per
spec section 13.2's own text: "Without telemetry, approximate by call
graph centrality."

Explicitly NOT attempted here (cut for scope, see
be-v2/docs/cie-code-intelligence-implementation.md): fine-grained
data-structure-operation complexity (hashmap O(1) vs list-scan O(n)),
missing-pagination detection (needs API-endpoint context this module
doesn't have), and repeated-string-concatenation detection (needs type
information this module doesn't have — a `+=` on a bare identifier is
just as likely numeric accumulation as string building, and a
low-precision signal here isn't worth shipping).
"""
from __future__ import annotations

import hashlib

from cie.models import Confidence, Node, NodeKind
from cie.repository import Repository
from cie.source_analysis import SourceIndex, call_names_in, calls_name, loop_nesting_depth, loop_nodes

# Call-name substrings that look like a database/network round trip —
# used by the N+1 check. Deliberately broad (a heuristic, not a parser
# that actually knows what a symbol resolves to): false positives are
# expected and documented, not silently assumed away.
_DB_CALL_HINTS = (
    "query", "execute", "fetch", "find", "select", "save", "commit",
    "session.", "cursor.", ".get(", "objects.", "collection.",
)

# Call-name substrings that look like synchronous I/O.
_SYNC_IO_HINTS = (
    "requests.", "urllib", "http.client", "socket.", "open(", "time.sleep",
)

_HOT_PATH_TOP_FRACTION = 0.1  # top 10% by degree, minimum 1


def _looks_like(name: str, hints: tuple[str, ...]) -> bool:
    lowered = name.lower()
    return any(h in lowered for h in hints)


def _is_tail_call(ts_node, own_name: str) -> bool:
    """True when `ts_node` (a function/method body) recurses via a call
    that is DIRECTLY the value of a `return` statement — the shape a
    tail-call-optimizable (O(n)) recursion has. Anything else recursive
    (the call feeds into an expression, e.g. `f(n-1) + f(n-2)`, or isn't
    in a return at all) is treated as naive branching recursion (O(2^n))
    — see this module's docstring: this is the spec's own suggested
    approximation, not a guarantee."""
    found = False

    def _walk(n) -> None:
        nonlocal found
        if found:
            return
        if n.type == "return_statement":
            for child in n.children:
                if child.type in ("call", "call_expression"):
                    fn = child.child_by_field_name("function")
                    if fn is not None:
                        text = fn.text.decode("utf-8", errors="replace")
                        if text == own_name or text.endswith(f".{own_name}"):
                            found = True
                            return
        for child in n.children:
            _walk(child)

    _walk(ts_node)
    return found


def estimate_complexity(ts_node, own_name: str) -> str:
    """CI-06: a single complexity-class label, combining loop nesting
    depth and a crude recursion signal. See module docstring for the
    documented approximation."""
    depth = loop_nesting_depth(ts_node)
    recursive = calls_name(ts_node, own_name)

    if recursive and depth == 0:
        return "O(n)" if _is_tail_call(ts_node, own_name) else "O(2^n)"
    if depth == 0:
        return "O(1)"
    if depth == 1:
        return "O(n)"
    if depth == 2:
        return "O(n^2)"
    return "O(n^3+)"


def detect_antipatterns(ts_node, own_name: str) -> list[dict]:
    """CI-07: `{pattern, severity, detail}` dicts for every anti-pattern
    found in `ts_node`'s span. `severity` is "critical"/"warning"/"info"
    per spec section 13.2's own vocabulary."""
    findings: list[dict] = []
    loops = loop_nodes(ts_node)

    for loop in loops:
        calls = call_names_in(loop)
        db_calls = [c for c in calls if _looks_like(c, _DB_CALL_HINTS)]
        if db_calls:
            findings.append({
                "pattern": "N_PLUS_ONE",
                "severity": "critical",
                "detail": f"possible N+1: {db_calls[0]}(...) called inside a loop",
            })
        io_calls = [c for c in calls if _looks_like(c, _SYNC_IO_HINTS)]
        if io_calls:
            findings.append({
                "pattern": "SYNC_IO_IN_LOOP",
                "severity": "warning",
                "detail": f"synchronous I/O inside a loop: {io_calls[0]}(...)",
            })

    if loop_nesting_depth(ts_node) >= 2:
        findings.append({
            "pattern": "NESTED_LOOPS",
            "severity": "warning",
            "detail": "loop nested inside another loop (O(n^2) or worse "
                       "over potentially large collections)",
        })

    if calls_name(ts_node, own_name) and not _is_tail_call(ts_node, own_name):
        findings.append({
            "pattern": "UNBOUNDED_RECURSION",
            "severity": "warning",
            "detail": "recursive call not in tail position; no base-case "
                       "guard verified by this pass (a crude signal — a "
                       "real base case that isn't a direct early return "
                       "is not detected, see module docstring)",
        })

    return findings


def analyze(repo: Repository, project: str = "") -> dict:
    """Run the full performance-analysis pass: CI-06 complexity_class +
    CI-08 hot_path patched onto existing FUNC/METHOD nodes, CI-07
    AntiPattern nodes written via `replace_analysis_nodes`.

    Never raises for "nothing found" — returns a summary with zero
    findings for an empty or clean project.
    """
    candidates = repo.code_symbol_nodes(project=project)
    if not candidates:
        repo.replace_analysis_nodes(NodeKind.ANTI_PATTERN.value, [], [], project=project)
        return {"analyzed": 0, "antipatterns": 0, "hot_paths": 0}

    source_index = SourceIndex()
    property_updates: dict[str, dict] = {}
    antipattern_nodes: list[dict] = []
    antipattern_edges: list[dict] = []
    analyzed = 0

    hot_ids = {
        nr.node.id
        for nr in repo.god_nodes(top_n=max(1, int(len(candidates) * _HOT_PATH_TOP_FRACTION)))
    }

    for node in candidates:
        ts_node = source_index.locate(node.source_file, node.line_start, node.kind)
        if ts_node is None:
            continue
        analyzed += 1
        complexity = estimate_complexity(ts_node, node.label)
        property_updates[node.id] = {
            "complexity_class": complexity,
            "hot_path": node.id in hot_ids,
        }
        for finding in detect_antipatterns(ts_node, node.label):
            finding_id = "antipattern::" + hashlib.sha1(
                f"{node.id}|{finding['pattern']}".encode("utf-8")
            ).hexdigest()[:16]
            antipattern_nodes.append({
                "id": finding_id,
                "label": f"{finding['pattern']} in {node.label}",
                "source_file": node.source_file,
                "source_location": node.source_location,
                "file_type": "analysis",
                "kind": NodeKind.ANTI_PATTERN.value,
                "signature": "",
                "line_start": node.line_start,
                "line_end": node.line_end,
                "docstring": "",
                "pattern": finding["pattern"],
                "severity": finding["severity"],
                "detail": finding["detail"],
            })
            antipattern_edges.append({
                "source": node.id,
                "target": finding_id,
                "relation": "HAS_ANTIPATTERN",
                "confidence": Confidence.INFERRED.value,
            })

    repo.update_node_properties(property_updates, project=project)
    repo.replace_analysis_nodes(
        NodeKind.ANTI_PATTERN.value, antipattern_nodes, antipattern_edges,
        project=project,
    )
    return {
        "analyzed": analyzed,
        "antipatterns": len(antipattern_nodes),
        "hot_paths": len(hot_ids),
    }
