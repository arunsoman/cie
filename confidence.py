"""CF-15/16: Confidence Scoring (spec section 14.7).

Pure composition over already-built layer signals — `cie.traceability`
(CF-08) and `cie.consensus` (CF-12/14) — no new extraction. The spec's
own five layers (spec, generation, validation, testing, runtime) are
narrowed to the three this codebase can actually measure today: `spec`
(contract coverage), `testing` (test coverage), `consensus` (agent
verdicts). `generation`/`runtime` layers are reported as `None` (not
`0.0`) — see `compute_confidence_report`'s docstring for why that
distinction matters, same reasoning `quality_report.freshness_report`
already applies to `never_stamped_count` vs `stale_count`.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Optional, Sequence

from cie.models import EdgeRecord, Node
from cie import traceability as _traceability

#: (min_score, level) pairs, checked in order — first match wins.
_LEVELS = [(0.9, "proven"), (0.7, "high"), (0.4, "medium"), (0.01, "low"), (0.0, "none")]


def _level_for(score: float) -> str:
    for threshold, label in _LEVELS:
        if score >= threshold:
            return label
    return "none"


def compute_confidence_report(
    nodes: Sequence[Node], edges: Sequence[EdgeRecord], verdicts: Sequence[dict],
) -> dict:
    """CF-15: aggregate `spec`/`testing`/`consensus` layer scores into an
    overall confidence level. `spec`/`testing` come from CF-08's
    `get_coverage` (`None`, not `0.0`, when there are no code symbols to
    measure yet — i.e. CIE hasn't scanned any code for this project);
    `consensus` from CF-14's `consensus_confidence` (`None`, not `0.0`,
    when `verdicts` is empty). SI-05's own framing of "empty layers show
    as N/A, not 0" applies to all three measurable layers, even before
    section 17's subsystem-population layer exists to enforce it
    everywhere. `generation`/`runtime` layers (per the spec's full
    5-layer model) are always `None` — this codebase has no signal for
    either yet (no code-generation-confidence tracking, no runtime
    telemetry ingestion; see `cie.invariants`' own scoping note on CI-15).
    """
    from cie import consensus as _consensus

    coverage = _traceability.get_coverage(nodes, edges)
    orphans = _traceability.find_orphans(nodes, edges)
    has_code = coverage["total_code_symbols"] > 0
    layer_scores: dict[str, Optional[float]] = {
        "spec": coverage["contracted_pct"] / 100.0 if has_code else None,
        "testing": coverage["tested_pct"] / 100.0 if has_code else None,
        "consensus": _consensus.consensus_confidence(verdicts) if verdicts else None,
        "generation": None,
        "runtime": None,
    }
    measured = [v for v in layer_scores.values() if v is not None]
    overall = round(sum(measured) / len(measured), 3) if measured else 0.0
    return {
        "overall_score": overall, "level": _level_for(overall),
        "layer_scores": layer_scores,
        "uncovered_spec_nodes": [n.id for n in orphans],
        "verdict_count": len(verdicts),
        "measured_at": datetime.now(timezone.utc).isoformat(),
    }
