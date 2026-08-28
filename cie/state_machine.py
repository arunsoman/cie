"""CF-06/07: State Machine Modeling (spec section 14.3).

`validate_code_against_fsm` is narrowed to a STRUCTURAL check: does a
code node with a matching label exist for each declared state? This is
a weak but real signal — NOT the reachability/model-checking the spec's
own caveat says needs an external model checker (NuSMV/SPIN), explicitly
called out there as separate, larger work. Basic FSM validation (no dead
states, no unreachable states — pure graph algorithms over the
transitions already extracted) IS in scope per that same caveat and is
also implemented here (`find_unreachable_states`/`find_dead_states`).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional, Sequence

from pydantic import BaseModel

from core.llm import LlmAgent, Prompt, ask
from cie.models import Node, NodeKind

# -- CF-06: StateMachineBuilder ------------------------------------------------


class StateItem(BaseModel):
    name: str
    description: str = ""


class TransitionItem(BaseModel):
    from_state: str
    to_state: str
    trigger: str
    guard: str = ""


class FsmLlmOutput(BaseModel):
    entity_name: str
    states: list[StateItem]
    transitions: list[TransitionItem]


@dataclass(frozen=True)
class FsmExtractPrompt(Prompt[FsmLlmOutput]):
    text: str

    def system_prompt(self) -> str:
        return (
            "You extract a FORMAL finite state machine for ONE stateful "
            "entity described in product requirement text: its states, "
            "and every valid transition between them (trigger + optional "
            "guard condition). If the text describes no stateful entity "
            "with distinct states, return an empty states list and an "
            "empty entity_name. Never invent a state or transition not "
            "grounded in the text."
        )

    def render(self) -> str:
        return f"## Requirement text\n{self.text}"


AGENT = LlmAgent(
    name="cie_fsm_builder", prompt_type=FsmExtractPrompt, output_type=FsmLlmOutput,
)


async def extract_state_machine(text: str) -> Optional[dict]:
    """CF-06: LLM-based FSM extraction. Returns None when the text
    describes nothing stateful (empty states list) rather than an
    empty-but-present FSM dict — a caller shouldn't have to check
    `fsm["states"]` itself to know whether extraction found anything."""
    if not text.strip():
        return None
    output = await ask(FsmExtractPrompt(text=text))
    if not output.states:
        return None
    return {
        "entity_name": output.entity_name,
        "states": [s.model_dump() for s in output.states],
        "transitions": [t.model_dump() for t in output.transitions],
    }


def build_fsm_nodes(fsm: dict) -> tuple[list[dict], list[dict]]:
    """Deterministic given `fsm` — ids are derived from `entity_name`/
    state name/transition index, so re-running on the same extracted FSM
    is idempotent (MERGE-safe)."""
    entity = fsm["entity_name"]
    sm_id = f"statemachine::{entity}"
    nodes: list[dict] = [{
        "id": sm_id, "label": entity, "kind": NodeKind.STATE_MACHINE.value, "source_file": "",
    }]
    edges: list[dict] = []
    state_ids: dict[str, str] = {}
    for s in fsm["states"]:
        sid = f"{sm_id}::state::{s['name']}"
        state_ids[s["name"]] = sid
        nodes.append({
            "id": sid, "label": s["name"], "kind": NodeKind.STATE.value, "source_file": "",
            "description": s.get("description", ""),
        })
        edges.append({"source": sm_id, "target": sid, "relation": "HAS_STATE", "confidence": "INFERRED"})
    for i, t in enumerate(fsm["transitions"]):
        tid = f"{sm_id}::transition::{i}"
        nodes.append({
            "id": tid, "label": f"{t['from_state']} -> {t['to_state']}",
            "kind": NodeKind.TRANSITION.value, "source_file": "",
            "from_state": t["from_state"], "to_state": t["to_state"],
            "trigger": t.get("trigger", ""), "guard": t.get("guard", ""),
        })
        edges.append({"source": sm_id, "target": tid, "relation": "HAS_TRANSITION", "confidence": "INFERRED"})
        if t["from_state"] in state_ids:
            edges.append({
                "source": state_ids[t["from_state"]], "target": tid,
                "relation": "TRANSITIONS_FROM", "confidence": "INFERRED",
            })
        if t["to_state"] in state_ids:
            edges.append({
                "source": tid, "target": state_ids[t["to_state"]],
                "relation": "TRANSITIONS_TO", "confidence": "INFERRED",
            })
    return nodes, edges


def validate_code_against_fsm(fsm: dict, code_nodes: Sequence[Node]) -> list[dict]:
    """CF-06's `validate_code_against_fsm`, structural-only (see module
    docstring): flags any declared state with no matching code node
    label (an enum member, a status-string constant, etc.)."""
    labels = {n.label for n in code_nodes}
    return [
        {"state": s["name"], "issue": "no matching code symbol/enum member found"}
        for s in fsm["states"] if s["name"] not in labels
    ]


def find_unreachable_states(fsm: dict) -> list[str]:
    """Basic FSM validation (explicitly in-scope per the spec's own
    caveat, unlike full reachability proofs): a state with no INCOMING
    transition AND that isn't the first declared state (treated as the
    initial state) is unreachable."""
    if not fsm["states"]:
        return []
    initial = fsm["states"][0]["name"]
    reachable_targets = {t["to_state"] for t in fsm["transitions"]}
    return [
        s["name"] for s in fsm["states"]
        if s["name"] != initial and s["name"] not in reachable_targets
    ]


def find_dead_states(fsm: dict) -> list[str]:
    """A state with no OUTGOING transition — code can enter it but never
    leave (may be intentional for a genuine terminal state; this is a
    report, not a verdict on whether it's a bug)."""
    sources = {t["from_state"] for t in fsm["transitions"]}
    return [s["name"] for s in fsm["states"] if s["name"] not in sources]
