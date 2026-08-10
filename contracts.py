"""CF-01/02/03: Contract Extraction & Enforcement (spec section 14.1).

Scope narrowed relative to the spec text:
- CF-01: only `formal_language: "python_assert"` is generated — Z3 SMT,
  SQL check, and TypeScript assert are explicitly listed as OTHER
  supported formal languages in the spec, none attempted here (Z3 in
  particular needs a solver-backed verification loop this pass doesn't
  build). Binding a contract to code is best-effort NAME matching
  against the PRD's own `scope` text — no code semantics are shown to
  the LLM, so this can't do better than that without a second pass.
- CF-02: `validate_types` only checks PARAMETER-NAME-based domain-type
  rules (a project declares "parameters named `email` should be typed
  `Email`, not `str`") — no whole-codebase blanket type substitution
  (that would flag every string parameter in the project). `inject_
  assertions`/`strip_assertions` do TEXTUAL insertion/removal (a marker
  comment makes stripping unambiguous), not a full `ast` rewrite — a
  round-trip through `ast.unparse` does not guarantee preserving a
  function's original formatting/comments.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from typing import Sequence

from pydantic import BaseModel

from core.llm import LlmAgent, Prompt, ask
from cie.data_model import _parse_signature_types, _clean_type_name
from cie.models import Node, NodeKind

# -- CF-01: ContractExtractor --------------------------------------------------


class ContractLlmItem(BaseModel):
    contract_type: str  # precondition | postcondition | invariant | guard
    scope: str  # the function/class/endpoint name this applies to
    expression: str  # a Python-boolean-expression suitable for `assert <expression>`
    source_clause: str  # the requirement text fragment this was derived from


class ContractLlmOutput(BaseModel):
    contracts: list[ContractLlmItem]


@dataclass(frozen=True)
class ContractExtractPrompt(Prompt[ContractLlmOutput]):
    text: str

    def system_prompt(self) -> str:
        return (
            "You extract FORMAL, VERIFIABLE contracts from product "
            "requirement text: pre-conditions, post-conditions, "
            "invariants, and guards. Each contract must name the "
            "function/class/endpoint it applies to (as literally named or "
            "clearly implied in the text) and express the rule as a "
            "single Python boolean expression suitable for "
            "`assert <expression>` (e.g. `retries <= 3`, "
            "`email is not None and '@' in email`). Only reference names "
            "that would plausibly be local variables/parameters in that "
            "function — never invent object attribute chains you cannot "
            "know exist. If the text states no verifiable rule, return an "
            "empty contracts list. Never invent a contract not grounded "
            "in the text."
        )

    def render(self) -> str:
        return f"## Requirement text\n{self.text}"


AGENT = LlmAgent(
    name="cie_contract_extractor", prompt_type=ContractExtractPrompt,
    output_type=ContractLlmOutput,
)


async def extract_contracts(text: str) -> list[dict]:
    """CF-01: LLM-based extraction of formal contracts from requirement
    text. `text.strip() == ""` short-circuits to `[]` without an LLM
    call (nothing to extract)."""
    if not text.strip():
        return []
    output = await ask(ContractExtractPrompt(text=text))
    return [
        {
            "contract_type": item.contract_type,
            "formal_language": "python_assert",
            "scope": item.scope,
            "source": "llm_inferred",
            "expression": item.expression,
            "source_clause": item.source_clause,
        }
        for item in output.contracts
    ]


def _stable_id(*parts: str) -> str:
    return hashlib.sha256("::".join(parts).encode()).hexdigest()[:16]


def bind_contracts_to_code(
    contracts: Sequence[dict], nodes: Sequence[Node],
) -> tuple[list[dict], list[dict]]:
    """Link extracted contracts to existing FUNC/METHOD/CLASS nodes by
    best-effort exact-label matching against each contract's `scope`.
    An ambiguous (multiple candidates) or unresolved `scope` still gets
    a `Contract` node (never dropped) but no `HAS_CONTRACT` edge —
    stays queryable/visible as "unbound" rather than silently discarded.
    Node ids are content-hashed (`_stable_id`), so re-extracting the
    SAME contract from the same text produces the SAME id — MERGE-safe,
    idempotent across repeated runs.
    """
    by_label: dict[str, list[str]] = {}
    for n in nodes:
        if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value, NodeKind.CLASS.value):
            by_label.setdefault(n.label, []).append(n.id)

    contract_nodes: list[dict] = []
    edges: list[dict] = []
    for c in contracts:
        contract_id = "contract::" + _stable_id(
            c["contract_type"], c["scope"], c["expression"],
        )
        contract_nodes.append({
            "id": contract_id, "label": f"{c['contract_type']}: {c['expression']}",
            "kind": NodeKind.CONTRACT.value, "source_file": "",
            "contract_type": c["contract_type"], "formal_language": c["formal_language"],
            "scope": c["scope"], "source": c["source"], "expression": c["expression"],
            "source_clause": c.get("source_clause", ""),
        })
        candidates = by_label.get(c["scope"], [])
        if len(candidates) == 1:
            edges.append({
                "source": candidates[0], "target": contract_id,
                "relation": "HAS_CONTRACT", "confidence": "INFERRED",
            })
    return contract_nodes, edges


def contracts_query(contract_nodes: Sequence[Node], scope: str = "", contract_type: str = "") -> list[Node]:
    """CF-03: filter already-loaded `Contract` nodes by scope/type."""
    out = list(contract_nodes)
    if scope:
        out = [n for n in out if n.properties.get("scope") == scope]
    if contract_type:
        out = [n for n in out if n.properties.get("contract_type") == contract_type]
    return out


# -- CF-02: SemanticSchemaEnforcer ---------------------------------------------

def validate_types(nodes: Sequence[Node], domain_types: dict[str, str]) -> list[dict]:
    """Static validation half: flag FUNC/METHOD parameters whose NAME is
    a key in `domain_types` (e.g. `{"email": "Email"}`) but whose
    ANNOTATED type doesn't match the expected domain type. Matched by
    parameter name, not a blanket type substitution — see module
    docstring."""
    violations: list[dict] = []
    for n in nodes:
        if n.kind not in (NodeKind.FUNC.value, NodeKind.METHOD.value) or not n.signature:
            continue
        _return_type, params = _parse_signature_types(n.signature)
        for pname, ptype in params:
            expected = domain_types.get(pname)
            if expected and _clean_type_name(ptype) != expected:
                violations.append({
                    "symbol_id": n.id, "param_name": pname,
                    "actual_type": _clean_type_name(ptype), "expected_type": expected,
                })
    return violations


_INJECTED_MARKER = "  # CF-02-INJECTED"
_DEF_RE = re.compile(r"^(\s*)def\s+(\w+)\s*\(.*\)\s*(->\s*.+)?:\s*$")


def inject_assertions(code: str, contracts: Sequence[dict]) -> str:
    """Assertion-injection half: insert `assert <expression>` (tagged
    with a unique marker comment) as the first line inside every
    function whose name matches a contract's `scope`. Textual insertion,
    not an AST rewrite — see module docstring."""
    scopes = {c["scope"]: c["expression"] for c in contracts if c.get("scope") and c.get("expression")}
    if not scopes:
        return code
    out: list[str] = []
    for line in code.split("\n"):
        out.append(line)
        match = _DEF_RE.match(line)
        if match and match.group(2) in scopes:
            indent = match.group(1) + "    "
            out.append(f"{indent}assert {scopes[match.group(2)]}{_INJECTED_MARKER}")
    return "\n".join(out)


def strip_assertions(code: str) -> str:
    """Remove every line `inject_assertions` added — identified solely
    by the marker comment, never a developer-written assert."""
    return "\n".join(
        line for line in code.split("\n") if not line.rstrip().endswith(_INJECTED_MARKER)
    )
