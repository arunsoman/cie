"""Section 16.1/16.4: Exhaustive Test Orchestration + Coverage Verification
(spec sections 16.1, 16.4) — TE-01/02/03/12/13/14.

Scope narrowed relative to the spec text:
- TE-01 (test plan): covers `InteractiveElement` (DE-02), `Contract`
  (CF-01), `Transition` (CF-06), API endpoints (`cie.api_routes.
  extract_backend_routes`), PRD error scenarios (`extract_error_
  scenarios`, LLM-based, same `core.llm` `Prompt`/`ask` pattern as CF-01's
  `ContractExtractor` — added as a follow-up, see `cie-test-execution-apm
  -implementation.md`), and domain-type boundary cases (`generate_
  boundary_tests`, rule-based over a small known-type table, also a
  follow-up). "Every cross-file compliance rule" is still not
  attempted — no structured source for that exists anywhere in this
  graph.
- TE-02 (multi-layer runner): only `unit`/`integration` layers actually
  EXECUTE (real subprocess pytest, via the same sandboxed `cie.tools.
  runner.run_command` PS-06's behavioral gate already uses). Every other
  layer (e2e/contract/mutation/fuzz/property/ui_regression/
  state_machine) is reported `not_executed` with a reason — running
  Playwright/mutmut/Hypothesis/pixel-diff tooling against a specific
  generated project's own test setup is out of scope for a generic
  graph-layer tool; faking their execution would be worse than being
  honest about the gap.
- TE-13 (nook-and-corner): generates ONE round of targeted test
  skeletons (reusing CF-04's `cie.test_synthesis`) for coverage gaps.
  The spec's "re-run, repeat until threshold" loop needs TE-02's real
  execution of the newly-generated tests, which for the layers this
  slice doesn't execute isn't meaningful to loop on.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Sequence

from pydantic import BaseModel

from core.llm import LlmAgent, Prompt, ask
from cie.models import Node, NodeKind


def _stable_id(*parts: str) -> str:
    return hashlib.sha256("::".join(parts).encode()).hexdigest()[:16]


# -- TE-01: Test Plan Generator ------------------------------------------------

def generate_test_plan(
    elements: Sequence[Node], contracts: Sequence[Node], transitions: Sequence[Node],
    repo_root: Optional[Path] = None,
) -> tuple[list[dict], list[dict]]:
    """TE-01: one `TestExecution` per coverable entity — see module
    docstring for the exact set. Returns
    `(test_execution_nodes, covers_edges)`; ids are content-hashed, so
    regenerating the plan for the same inputs is idempotent."""
    nodes: list[dict] = []
    edges: list[dict] = []

    def _add(target_id: str, test_type: str, coverage_scope: str) -> None:
        exec_id = "testexec::" + _stable_id(target_id, test_type)
        nodes.append({
            "id": exec_id, "label": f"{test_type}: {coverage_scope}",
            "kind": NodeKind.TEST_EXECUTION.value, "source_file": "",
            "target_id": target_id, "test_type": test_type,
            "coverage_scope": coverage_scope, "status": "pending",
        })
        edges.append({
            "source": exec_id, "target": target_id,
            "relation": "COVERS", "confidence": "INFERRED",
        })

    for el in elements:
        _add(el.id, "ui_element", el.properties.get("element_kind", el.label))
    for c in contracts:
        _add(c.id, "contract", c.properties.get("scope", c.label))
    for t in transitions:
        _add(t.id, "state_machine", t.label)

    if repo_root is not None:
        from cie import api_routes

        for route in api_routes.extract_backend_routes(Path(repo_root)):
            route_id = f"apiroute::{route.method}::{route.normalized_path}"
            _add(route_id, "api_endpoint", f"{route.method} {route.path}")

    return nodes, edges


# -- TE-01, follow-up: PRD error scenarios (LLM) ------------------------------

class ErrorScenarioItem(BaseModel):
    description: str
    trigger_condition: str
    target_label: str  # the function/endpoint this scenario applies to, as named/implied in the text


class ErrorScenarioLlmOutput(BaseModel):
    scenarios: list[ErrorScenarioItem]


@dataclass(frozen=True)
class ErrorScenarioPrompt(Prompt[ErrorScenarioLlmOutput]):
    text: str

    def system_prompt(self) -> str:
        return (
            "You extract ERROR SCENARIOS from product requirement text: "
            "situations where something goes wrong (invalid input, a "
            "downstream failure, a permission denial, a timeout, a "
            "race condition) that the described function/endpoint must "
            "handle. For each, name the function/class/endpoint it "
            "applies to (as literally named or clearly implied in the "
            "text), the triggering condition, and a one-sentence "
            "description of the scenario. If the text describes no "
            "error handling requirement, return an empty scenarios "
            "list. Never invent a scenario not grounded in the text."
        )

    def render(self) -> str:
        return f"## Requirement text\n{self.text}"


AGENT = LlmAgent(
    name="cie_error_scenario_extractor", prompt_type=ErrorScenarioPrompt,
    output_type=ErrorScenarioLlmOutput,
)


async def extract_error_scenarios(text: str) -> list[dict]:
    """TE-01 follow-up: LLM-based extraction of error scenarios from
    requirement text — the same `core.llm` `Prompt`/`ask` pattern CF-01's
    `ContractExtractor` already established. `text.strip() == ""`
    short-circuits to `[]` without an LLM call."""
    if not text.strip():
        return []
    output = await ask(ErrorScenarioPrompt(text=text))
    return [item.model_dump() for item in output.scenarios]


def build_error_scenario_tests(
    scenarios: Sequence[dict], nodes: Sequence[Node],
) -> tuple[list[dict], list[dict]]:
    """TE-01 follow-up: one `TestExecution` (`test_type="error_scenario"`)
    per extracted scenario. Binds to an existing FUNC/METHOD/CLASS node
    by exact label match against `target_label`, same best-effort
    approach `cie.contracts.bind_contracts_to_code` uses — an
    unresolved/ambiguous target still gets a `TestExecution` node (never
    dropped), just no `COVERS` edge."""
    by_label = {
        n.label: n.id for n in nodes
        if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value, NodeKind.CLASS.value)
    }
    exec_nodes: list[dict] = []
    edges: list[dict] = []
    for scenario in scenarios:
        exec_id = "testexec::" + _stable_id(
            scenario["target_label"], "error_scenario", scenario["trigger_condition"],
        )
        exec_nodes.append({
            "id": exec_id, "label": f"error_scenario: {scenario['description']}",
            "kind": NodeKind.TEST_EXECUTION.value, "source_file": "",
            "target_id": scenario["target_label"], "test_type": "error_scenario",
            "coverage_scope": scenario["trigger_condition"], "status": "pending",
        })
        target_id = by_label.get(scenario["target_label"])
        if target_id:
            edges.append({
                "source": exec_id, "target": target_id,
                "relation": "COVERS", "confidence": "INFERRED",
            })
    return exec_nodes, edges


# -- TE-01, follow-up: domain-type boundary cases (rule-based) ---------------

#: type name -> its known boundary/edge cases. Deliberately small — the
#: common domain types this codebase's own `cie.contracts.validate_types`
#: examples use (Email, Money) plus Python/TS builtins. Anything else
#: falls back to `_DEFAULT_BOUNDARY_HINTS` rather than generating
#: nothing for an unrecognized type name.
_BOUNDARY_RULES: dict[str, list[str]] = {
    "str": ["empty string", "very long string (10000+ chars)", "unicode/emoji", "leading/trailing whitespace"],
    "string": ["empty string", "very long string (10000+ chars)", "unicode/emoji", "leading/trailing whitespace"],
    "int": ["zero", "negative value", "integer overflow boundary"],
    "number": ["zero", "negative value", "integer overflow boundary"],
    "float": ["zero", "negative value", "NaN", "infinity"],
    "list": ["empty list", "single element", "very large list"],
    "bool": ["explicit False vs missing/None"],
    "Email": ["missing @", "missing domain", "empty string", "excessively long local part"],
    "Money": ["zero", "negative value", "excessively large value", "sub-cent precision"],
}
_DEFAULT_BOUNDARY_HINTS = ["empty/null value", "boundary-adjacent value"]


def generate_boundary_tests(domain_types: dict[str, str]) -> tuple[list[dict], list[dict]]:
    """TE-01 follow-up: for each `{param_name: type_name}` pair (the
    SAME shape `cie.contracts.validate_types`'s `domain_types` argument
    uses), one `TestExecution` (`test_type="boundary_value"`) per known
    boundary case for that type. No `COVERS` edges — `param_name` is a
    bare parameter name, not a resolvable graph node id (ambiguous
    across the codebase; a real binding would need the specific function
    signature it came from, which this rule-based generator doesn't have).
    """
    nodes: list[dict] = []
    for param_name, type_name in domain_types.items():
        for hint in _BOUNDARY_RULES.get(type_name, _DEFAULT_BOUNDARY_HINTS):
            exec_id = "testexec::" + _stable_id(param_name, type_name, "boundary_value", hint)
            nodes.append({
                "id": exec_id, "label": f"boundary: {param_name} ({type_name}) — {hint}",
                "kind": NodeKind.TEST_EXECUTION.value, "source_file": "",
                "target_id": param_name, "test_type": "boundary_value",
                "coverage_scope": hint, "status": "pending",
            })
    return nodes, []


# -- TE-02: Multi-Layer Test Runner (narrowed) --------------------------------

_EXECUTED_LAYERS = frozenset({"unit", "integration"})
_PLANNED_ONLY_LAYERS: dict[str, str] = {
    "ui_element": "not a runnable layer name — plan-generation category only",
    "contract": "requires a running SmartMockServer instance (TE-05)",
    "e2e": "requires Playwright/Cypress wired into the target project",
    "mutation": "requires mutmut or equivalent mutation-testing tooling",
    "fuzz": "requires an adversarial-input generator (e.g. Atheris/Hypothesis)",
    "property": "requires Hypothesis/QuickCheck wired into the target project",
    "ui_regression": "requires a pixel-diff baseline against wireframes",
    "state_machine": "requires a transition-coverage harness driving real code",
    "api_endpoint": "requires a live server + HTTP client, not a bare subprocess",
    "error_scenario": "not a runnable layer name — plan-generation category only",
    "boundary_value": "not a runnable layer name — plan-generation category only",
}


def run_test_layer(
    test_type: str, target_files: Sequence[str], project_root: Path, timeout: int = 300,
) -> dict:
    """TE-02: execute ONE layer for real (`unit`/`integration` only —
    see module docstring). Every other layer returns `{"status":
    "not_executed", "reason": ...}` without attempting anything."""
    if test_type not in _EXECUTED_LAYERS:
        reason = _PLANNED_ONLY_LAYERS.get(test_type, f"unknown test layer {test_type!r}")
        return {"test_type": test_type, "status": "not_executed", "reason": reason}
    if not target_files:
        return {"test_type": test_type, "status": "not_executed", "reason": "no target files given"}

    from cie.tools.runner import run_command

    cmd = "python -m pytest -q " + " ".join(f'"{f}"' for f in target_files)
    try:
        result = run_command(cmd, cwd=project_root, timeout=timeout, allowed_root=project_root)
    except Exception as exc:  # noqa: BLE001 - infra failure, reported not raised
        return {"test_type": test_type, "status": "error", "reason": str(exc)}
    if result.timed_out:
        return {"test_type": test_type, "status": "timed_out", "output": result.output[:2000]}
    status = "passed" if result.exit_code == 0 else "failed"
    return {
        "test_type": test_type, "status": status,
        "exit_code": result.exit_code, "output": result.output[:2000],
    }


# -- TE-03: Test Result Aggregator --------------------------------------------

def aggregate_results(test_executions: Sequence[Node], results: dict[str, dict]) -> dict:
    """TE-03: join `TestExecution` nodes with per-execution results
    (keyed by `TestExecution` id) into a unified matrix: entity -> test
    type -> result, plus a status-count summary."""
    matrix = []
    for te in test_executions:
        result = results.get(te.id, {"status": "pending"})
        matrix.append({
            "test_execution_id": te.id, "target_id": te.properties.get("target_id", ""),
            "test_type": te.properties.get("test_type", ""), "status": result.get("status", "pending"),
        })
    counts: dict[str, int] = {}
    for row in matrix:
        counts[row["status"]] = counts.get(row["status"], 0) + 1
    return {"matrix": matrix, "counts": counts}


# -- TE-12: Coverage Gap Detector ---------------------------------------------

def coverage_gaps(test_executions: Sequence[Node], results: dict[str, dict]) -> list[dict]:
    """TE-12: every `TestExecution` whose result status isn't
    `"passed"`, categorized by `test_type`."""
    gaps = []
    for te in test_executions:
        status = results.get(te.id, {}).get("status", "pending")
        if status == "passed":
            continue
        gaps.append({
            "test_execution_id": te.id, "target_id": te.properties.get("target_id", ""),
            "test_type": te.properties.get("test_type", ""), "status": status,
        })
    return gaps


# -- TE-13: Nook-and-Corner Testing (one round) -------------------------------

def nook_and_corner_tests(gaps: Sequence[dict]) -> tuple[list[dict], list[dict]]:
    """TE-13, ONE ROUND (see module docstring): a targeted test skeleton
    per gap, reusing `cie.test_synthesis`'s templates (falling back to
    `"unit"` for a gap `test_type` outside its six templates, e.g.
    `"api_endpoint"`). No `TESTS` edges are created — a gap's target may
    be a UI element/contract/endpoint, not necessarily a code symbol
    `test_synthesis.build_test_nodes`'s label-matching binding expects.
    """
    from cie import test_synthesis

    specs = []
    for gap in gaps:
        test_type = gap["test_type"] if gap["test_type"] in test_synthesis.VALID_TEST_TYPES else "unit"
        specs.append({
            "criteria_text": f"coverage gap: {gap['test_execution_id']} (status={gap['status']})",
            "target_label": gap["target_id"], "test_type": test_type,
            "spec_node_id": gap["test_execution_id"],
        })
    return test_synthesis.build_test_nodes(specs, [])


def write_skeleton_files(
    skeleton_nodes: Sequence[dict], project_root: Path, subdir: str = "tests/generated",
) -> list[str]:
    """TE-13 follow-up: writes each generated skeleton's `skeleton_code`
    to a REAL file under `project_root/subdir` — not just a graph node.
    A skip-marked skeleton (CF-04's own deliberate scoping — no
    LLM-authored assertions) can never auto-resolve a gap on its own;
    the concrete, real value this adds is giving a human or agent an
    ACTUAL file to open and fill in, rather than only a graph record
    they'd have to materialize by hand first. Filenames are derived
    from each node's own content-hashed id, so regenerating for the
    SAME gap overwrites the same file rather than piling up duplicates.
    Directories are created as needed; an unwritable `project_root`
    raises (this is an explicit disk-write action, not something that
    should degrade silently). Returns the written paths, relative to
    `project_root`.
    """
    out_dir = Path(project_root) / subdir
    out_dir.mkdir(parents=True, exist_ok=True)
    written: list[str] = []
    for node in skeleton_nodes:
        filename = f"test_{node['id'].rsplit('::', 1)[-1]}.py"
        path = out_dir / filename
        path.write_text(node.get("skeleton_code", ""))
        written.append(str(path.relative_to(project_root)))
    return written


# -- TE-14: Unified Coverage Report --------------------------------------------

def unified_coverage_report(
    line_coverage: Optional[dict] = None,
    element_coverage_results: Sequence[dict] = (),
    traceability: Optional[dict] = None,
    mock_coverage_result: Optional[dict] = None,
    fsm_violation_count: Optional[int] = None,
    performance_regression_count: int = 0,
) -> dict:
    """TE-14: one report combining every coverage dimension this
    codebase already computes elsewhere. Does NO new measurement — pure
    assembly of what a caller already has from `coverage_report` (line),
    `element_coverage` (DE-06), `traceability_coverage` (CF-08/09),
    `mock_coverage` (TE-06), `fsm_validate` (CF-06/07),
    `performance_regressions` (TE-10/11)."""
    return {
        "dimensions": {
            "line_coverage": line_coverage,
            "element_coverage": {
                "total": sum(e.get("total_elements", 0) for e in element_coverage_results),
                "covered": sum(e.get("covered_count", 0) for e in element_coverage_results),
            },
            "spec_traceability": traceability,
            "mock_coverage": mock_coverage_result,
            "fsm_violation_count": fsm_violation_count,
            "performance_regression_count": performance_regression_count,
        },
    }
