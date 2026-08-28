"""CF-04/05: Acceptance Test Generation & Binding (spec section 14.2).

`bind_to_code` creates `TESTS` edges — the SAME edge type DM-14 already
defines and `cie.testlink` already resolves for real test files, per the
spec's own note that CF-04 "implements DM-14". No second edge type
invented for "generated skeleton tests" vs "real discovered tests"; a
`TestSkeleton` node's `status` property (skeleton/stubbed/implemented/
passing/failing) is what distinguishes a generated stub from a real,
already-passing test, not the edge type.

Scope narrowed: skeleton bodies are template-generated (unit/integration/
e2e/property/contract/state_machine — six `test_type`s, one template
each), not LLM-authored — the spec explicitly separates this from the
LLM prose-filling step (Requirement-Miner's job, external to CIE).
"""

from __future__ import annotations

import hashlib
import re
from typing import Sequence

from cie.models import Node, NodeKind

_TEMPLATES: dict[str, str] = {
    "unit": (
        'def test_{name}():\n'
        '    """{criteria}"""\n'
        '    pytest.skip("generated skeleton — not yet implemented")\n'
    ),
    "integration": (
        'def test_{name}_integration():\n'
        '    """{criteria}"""\n'
        '    pytest.skip("generated integration skeleton — not yet implemented")\n'
    ),
    "e2e": (
        'async def test_{name}_e2e(page):\n'
        '    """{criteria}"""\n'
        '    pytest.skip("generated E2E skeleton — not yet implemented")\n'
    ),
    "property": (
        '@given(st.data())\n'
        'def test_{name}_property(data):\n'
        '    """{criteria}"""\n'
        '    pytest.skip("generated property-test skeleton — not yet implemented")\n'
    ),
    "contract": (
        'def test_{name}_contract():\n'
        '    """{criteria}"""\n'
        '    pytest.skip("generated contract-test skeleton — not yet implemented")\n'
    ),
    "state_machine": (
        'def test_{name}_transitions():\n'
        '    """{criteria}"""\n'
        '    pytest.skip("generated state-machine-test skeleton — not yet implemented")\n'
    ),
}

VALID_TEST_TYPES = frozenset(_TEMPLATES)


def generate_test_skeleton(criteria_text: str, target_label: str, test_type: str = "unit") -> str:
    """CF-04: a runnable (but `pytest.skip`ped) test file body for
    `target_label`, templated by `test_type`. Unknown `test_type` falls
    back to `"unit"` rather than raising — a caller passing a typo'd
    type still gets something usable."""
    template = _TEMPLATES.get(test_type, _TEMPLATES["unit"])
    name = re.sub(r"[^0-9a-zA-Z_]", "_", target_label.strip().lower()) or "symbol"
    criteria = criteria_text.strip() or "no acceptance criteria text provided"
    return template.format(name=name, criteria=criteria)


def _stable_id(*parts: str) -> str:
    return hashlib.sha256("::".join(parts).encode()).hexdigest()[:16]


def build_test_nodes(
    specs: Sequence[dict], nodes: Sequence[Node],
) -> tuple[list[dict], list[dict]]:
    """CF-04: build `TestSkeleton` nodes + `TESTS` edges from a batch of
    specs, each `{criteria_text, target_label, test_type, spec_node_id}`.
    `target_label` is matched against existing FUNC/METHOD/CLASS node
    labels (exact match); no match still creates the TestSkeleton node
    (never dropped) with no `TESTS` edge. Ids are content-hashed, so
    re-running on the same specs is idempotent (MERGE-safe).
    """
    by_label = {
        n.label: n.id for n in nodes
        if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value, NodeKind.CLASS.value)
    }
    test_nodes: list[dict] = []
    edges: list[dict] = []
    for spec in specs:
        test_type = spec.get("test_type", "unit")
        if test_type not in VALID_TEST_TYPES:
            test_type = "unit"
        criteria_text = spec.get("criteria_text", "")
        target_label = spec["target_label"]
        skeleton = generate_test_skeleton(criteria_text, target_label, test_type)
        test_id = "testskeleton::" + _stable_id(target_label, test_type, criteria_text)
        test_nodes.append({
            "id": test_id, "label": f"test_{target_label}",
            "kind": NodeKind.TEST_SKELETON.value, "source_file": "",
            "test_type": test_type, "status": "skeleton",
            "skeleton_code": skeleton, "spec_node_id": spec.get("spec_node_id", ""),
        })
        target_id = by_label.get(target_label)
        if target_id:
            edges.append({
                "source": test_id, "target": target_id,
                "relation": "TESTS", "confidence": "INFERRED",
            })
    return test_nodes, edges


def test_coverage(spec_node_id: str, test_nodes: Sequence[Node]) -> dict:
    """CF-05: is there a generated test skeleton for `spec_node_id`?
    Returns `{spec_node_id, has_test, test_ids, statuses}`."""
    matches = [n for n in test_nodes if n.properties.get("spec_node_id") == spec_node_id]
    return {
        "spec_node_id": spec_node_id,
        "has_test": bool(matches),
        "test_ids": [n.id for n in matches],
        "statuses": [n.properties.get("status", "") for n in matches],
    }
