"""CF-19/20/21: Runtime Invariant Monitoring + Telemetry Backflow (spec
sections 14.9/14.10).

Narrowed heavily: this codebase has NO live production telemetry
pipeline (CI-15, "Telemetry Ingestion Pipeline", is itself "Missing" in
the spec's own table) to feed a real always-on monitor. What's built
here is the CHECKING PRIMITIVE — given one contract expression and one
state snapshot (a plain dict a caller already has from wherever it came
from), safely evaluate it and record a violation. This is NOT a
scheduler that watches production traffic, and there is no
`FeatureFlagManager` auto-rollback wiring (that system doesn't exist in
this codebase either). CF-21 similarly only builds the graph TRAVERSAL
(given a code node id, trace back to its contracts/tests) — the "on a
real runtime error" trigger itself needs CI-15, not attempted.
"""

from __future__ import annotations

import ast
import builtins as _builtins
from datetime import datetime, timezone
from typing import Any, Optional, Sequence

from cie.models import EdgeRecord, NodeKind

#: AST node types allowed in a contract expression — comparisons,
#: boolean/unary ops, names, literals, and calls to a small allowlist of
#: safe builtins. A contract expression is UNTRUSTED input (CF-01's LLM
#: extraction, or a human editing one) — never eval()'d with Python's
#: full builtins, and rejected BEFORE evaluation if it contains anything
#: outside this allowlist (attribute access, imports, arbitrary calls),
#: not caught after the fact.
_ALLOWED_NODES = (
    ast.Expression, ast.BoolOp, ast.BinOp, ast.UnaryOp, ast.Compare,
    ast.Name, ast.Load, ast.Constant, ast.And, ast.Or, ast.Not,
    ast.Eq, ast.NotEq, ast.Lt, ast.LtE, ast.Gt, ast.GtE, ast.In, ast.NotIn,
    ast.Is, ast.IsNot, ast.Call, ast.List, ast.Tuple,
    ast.Add, ast.Sub, ast.Mult, ast.Div, ast.USub, ast.UAdd,
)
_ALLOWED_CALLS = frozenset({"len", "abs", "min", "max"})
_ALLOWED_BUILTINS = {name: getattr(_builtins, name) for name in _ALLOWED_CALLS}


def _validate_ast(tree: ast.AST) -> None:
    for node in ast.walk(tree):
        if not isinstance(node, _ALLOWED_NODES):
            raise ValueError(f"disallowed expression element: {type(node).__name__}")
        if isinstance(node, ast.Call):
            if not isinstance(node.func, ast.Name) or node.func.id not in _ALLOWED_CALLS:
                raise ValueError("disallowed function call in contract expression")


def evaluate_contract(expression: str, state: dict[str, Any]) -> bool:
    """CF-19: safely evaluate a contract expression against a state
    snapshot. Raises `ValueError`/`SyntaxError` for anything unsafe or
    malformed rather than silently returning `True`/`False` — a contract
    that can't be checked should fail loudly to its caller, not be
    treated as passing."""
    tree = ast.parse(expression, mode="eval")
    _validate_ast(tree)
    code = compile(tree, "<contract>", "eval")
    return bool(eval(code, {"__builtins__": _ALLOWED_BUILTINS}, dict(state)))  # noqa: S307


def check_invariant(
    contract_id: str, expression: str, code_node_id: str, state: dict[str, Any],
    now: Optional[str] = None,
) -> Optional[dict]:
    """Evaluate one contract against one state snapshot. Returns an
    `InvariantViolation` node dict on failure, `None` when the contract
    holds OR when the expression itself is unsafe/unevaluable — an
    unevaluable contract is a data-quality problem to fix at the source
    (CF-01's extraction), not a runtime violation to report."""
    try:
        holds = evaluate_contract(expression, state)
    except Exception:  # noqa: BLE001 - malformed/unsafe expression
        return None
    if holds:
        return None
    timestamp = now or datetime.now(timezone.utc).isoformat()
    violation_id = f"violation::{contract_id}::{timestamp}"
    return {
        "id": violation_id, "label": f"violation of {contract_id}",
        "kind": NodeKind.INVARIANT_VIOLATION.value, "source_file": "",
        "contract_id": contract_id, "code_node_id": code_node_id,
        "violation_detail": f"expression {expression!r} evaluated False for state {state!r}",
        "timestamp": timestamp, "severity": "warning",
    }


def telemetry_to_spec(code_node_id: str, edges: Sequence[EdgeRecord]) -> dict:
    """CF-21: given a code node ALREADY IDENTIFIED as erroring (see
    module docstring for why that identification step isn't built here),
    trace back through already-existing `HAS_CONTRACT`/`TESTS` edges:
    code -> contract(s) -> test(s)."""
    contracts = [
        er.edge.target for er in edges
        if er.edge.relation == "HAS_CONTRACT" and er.edge.source == code_node_id
    ]
    tests = [
        er.edge.source for er in edges
        if er.edge.relation == "TESTS" and er.edge.target == code_node_id
    ]
    return {"code_node_id": code_node_id, "contracts": contracts, "tests": tests}
