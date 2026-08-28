"""CF-17/18: Justification & Proof (spec section 14.8).

Builds a human-readable proof-chain string from already-existing
traceability data (CF-08's `traceability_chain`) plus agent verdicts
(CF-12) — no new extraction. `formal_check` is always `None`: a real
"z3-sat"-style formal check would need an actual solver wired to a
contract's expression, which CF-19's `evaluate_contract` deliberately
implements as a restricted `ast`-based evaluator, not a theorem prover —
see that module's own scoping note.
"""

from __future__ import annotations

from typing import Sequence

from cie.models import EdgeRecord, Node
from cie import traceability as _traceability


def build_justification(
    symbol_id: str, nodes: Sequence[Node], edges: Sequence[EdgeRecord],
    verdicts: Sequence[dict],
) -> dict:
    """CF-17: `{symbol_id, proof_steps: [...], formal_check: None}` — a
    plain-English chain built from whatever traceability/consensus data
    already exists for `symbol_id`. Always includes a step even when
    nothing was found ("NO tests found...", "no agent verdicts
    recorded...") rather than omitting the line — an empty proof chain
    would look like a bug, not like genuinely "nothing verifies this."
    """
    chain = _traceability.traceability_chain(nodes, edges, symbol_id)
    steps: list[str] = []
    if chain["contracts"]:
        steps.append(f"{len(chain['contracts'])} contract(s) declared for this symbol")
    else:
        steps.append("no contracts declared for this symbol")
    if chain["tested_by"]:
        preview = ", ".join(chain["tested_by"][:3])
        steps.append(f"tested by {len(chain['tested_by'])} test(s): {preview}")
    else:
        steps.append("NO tests found linking to this symbol")
    if verdicts:
        steps.append(f"agent consensus: {len(verdicts)} verdict(s) recorded")
    else:
        steps.append("no agent verdicts recorded yet")
    return {"symbol_id": symbol_id, "proof_steps": steps, "formal_check": None}
