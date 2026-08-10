"""CF-12/13/14: Multi-Agent Consensus (spec section 14.6).

CF-13 (ConsensusBus — a durable, exactly-once, ordered agent-to-agent
message bus) is DELIBERATELY NOT built here. The spec's own caveat is
explicit: "exactly-once delivery requires idempotent consumers +
transactional outbox... reference implementation: NATS JetStream or
Kafka. Do not build from scratch." Nothing in this module attempts a
message bus; this module covers CF-12 (verdict storage) and CF-14
(query) only, receiving one verdict at a time through a plain function
call — a caller that already has a verdict from wherever its agents ran.
Wiring an actual durable bus in front of `build_verdict_node` is a
separate, later integration against a real broker.
"""

from __future__ import annotations

import hashlib
from datetime import datetime, timezone
from typing import Sequence

from cie.models import NodeKind

#: verdict -> weight when computing consensus confidence. A single
#: "fail" pulls the consensus down hard (weight 0.0) — one dissenting
#: prover should matter more than being outvoted by agreeable agents.
_VERDICT_WEIGHT = {"pass": 1.0, "needs_review": 0.5, "fail": 0.0}


def build_verdict_node(
    code_node_id: str, agent_name: str, verdict: str, confidence: float,
    reasoning: str, timestamp: str = "",
) -> tuple[dict, dict]:
    """CF-12: one `AgentVerdict` node + its `VERDICT_ON` edge.
    `verdict` (pass/fail/needs_review) is stored verbatim, not enforced
    here — the agent vocabulary belongs to the caller, not this storage
    layer. `timestamp` defaults to now; passing one explicitly makes the
    resulting id reproducible for tests."""
    timestamp = timestamp or datetime.now(timezone.utc).isoformat()
    verdict_id = "verdict::" + hashlib.sha256(
        f"{code_node_id}::{agent_name}::{timestamp}".encode()
    ).hexdigest()[:16]
    node = {
        "id": verdict_id, "label": f"{agent_name}: {verdict}",
        "kind": NodeKind.AGENT_VERDICT.value, "source_file": "",
        "agent_name": agent_name, "verdict": verdict, "confidence": confidence,
        "reasoning": reasoning, "timestamp": timestamp,
    }
    edge = {
        "source": verdict_id, "target": code_node_id,
        "relation": "VERDICT_ON", "confidence": "EXTRACTED",
    }
    return node, edge


def consensus_confidence(verdicts: Sequence[dict]) -> float:
    """CF-14: mean confidence across `verdicts`, each weighted by
    `_VERDICT_WEIGHT`. `0.0` (not an error) when `verdicts` is empty —
    no verdicts recorded is itself a meaningful "no consensus yet" state."""
    if not verdicts:
        return 0.0
    scored = [
        v.get("confidence", 0.0) * _VERDICT_WEIGHT.get(v.get("verdict", ""), 0.5)
        for v in verdicts
    ]
    return round(sum(scored) / len(scored), 3)
