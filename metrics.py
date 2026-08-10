"""CI-19/20/21: Metric Computer.

Reads whatever `cie.clone_detect`/`cie.perf_analyze`/`cie.drift_detect`
have ALREADY written to the graph and rolls them into three aggregate
metrics — it does NOT re-run those (more expensive) passes itself.
Running `metrics.compute()` against a project that has never run
`clone_detect_run`/`performance_analyze_run`/`drift_detect_run` is valid
and simply yields zeros for whichever inputs are missing (this module
has no way to distinguish "genuinely clean" from "never analyzed" on its
own — `cie.system_intelligence`-style population tracking, spec section
17, is explicitly out of scope for this slice; a `0` here should be read
as "nothing found OR nothing run", and the caller is expected to already
know which via its own workflow).

Metrics (spec section 13.5's own vocabulary, minus `refactor_score` —
cut for scope: it needs a real hot-path/complexity weighting formula
this module doesn't have a principled way to calibrate yet, and
`clone_coverage_pct` + `tech_debt_score` already surface the same signal
at lower risk of a made-up number):

- `clone_coverage_pct` (CI-05, surfaced here too for trend tracking):
  % of FUNC/METHOD symbols that belong to at least one CloneCluster.
- `drift_index`: severity-weighted sum of DriftFinding nodes
  (critical=3, warning=2, info=1), normalized to 0-100 against the
  project's own symbol count so it's comparable across project sizes
  (NOT an absolute count, which would make a big project look
  perpetually worse than a small one for the same defect density).
- `tech_debt_score`: a documented, simple weighted composite of
  clone_coverage_pct + drift_index + (anti-pattern count, same
  normalization as drift_index) — NOT a validated/calibrated formula,
  a starting point meant to be tuned against real data, exactly as
  `CI-06`'s Big-O estimate is documented as approximate rather than
  guaranteed.
"""
from __future__ import annotations

from datetime import datetime, timezone

from cie.models import MetricSnapshot, NodeKind
from cie.repository import Repository

_SEVERITY_WEIGHT = {"critical": 3, "warning": 2, "info": 1}

# tech_debt_score composite weights — each input is already 0-100
# normalized, so these sum to 1.0 and the result stays in 0-100 too.
_TECH_DEBT_WEIGHTS = {"clone_coverage_pct": 0.3, "drift_index": 0.4, "antipattern_index": 0.3}


def _normalize_count(count: int, total_symbols: int) -> float:
    if total_symbols <= 0:
        return 0.0
    return round(min(100.0, (count / total_symbols) * 100), 2)


def compute(repo: Repository, project: str = "") -> dict:
    """CI-19: compute clone_coverage_pct/drift_index/tech_debt_score
    from whatever section-13 analysis nodes already exist, write one
    `MetricSnapshot` per metric (append-only — CI-21's trend data), and
    return the readings plus a `measured_at` timestamp."""
    total_symbols = len(repo.code_symbol_nodes(project=project))

    clone_clusters = repo.analysis_nodes(NodeKind.CLONE_CLUSTER.value, project=project)
    clustered_symbols = sum(
        int(c.properties.get("member_count", 0)) for c in clone_clusters
    )
    clone_coverage_pct = _normalize_count(clustered_symbols, total_symbols)

    drift_findings = repo.analysis_nodes(NodeKind.DRIFT_FINDING.value, project=project)
    drift_weight = sum(
        _SEVERITY_WEIGHT.get(f.properties.get("severity", ""), 1) for f in drift_findings
    )
    drift_index = _normalize_count(drift_weight, total_symbols)

    antipatterns = repo.analysis_nodes(NodeKind.ANTI_PATTERN.value, project=project)
    antipattern_weight = sum(
        _SEVERITY_WEIGHT.get(a.properties.get("severity", ""), 1) for a in antipatterns
    )
    antipattern_index = _normalize_count(antipattern_weight, total_symbols)

    tech_debt_score = round(
        clone_coverage_pct * _TECH_DEBT_WEIGHTS["clone_coverage_pct"]
        + drift_index * _TECH_DEBT_WEIGHTS["drift_index"]
        + antipattern_index * _TECH_DEBT_WEIGHTS["antipattern_index"],
        2,
    )

    measured_at = datetime.now(timezone.utc).isoformat()
    readings = {
        "clone_coverage_pct": clone_coverage_pct,
        "drift_index": drift_index,
        "tech_debt_score": tech_debt_score,
    }
    for metric_type, value in readings.items():
        repo.record_metric_snapshot(MetricSnapshot(
            project=project, metric_type=metric_type, value=value,
            measured_at=measured_at,
            detail={
                "total_symbols": total_symbols,
                "clone_clusters": len(clone_clusters),
                "drift_findings": len(drift_findings),
                "antipatterns": len(antipatterns),
            },
        ))
    return {**readings, "measured_at": measured_at, "total_symbols": total_symbols}


def tech_debt_report(repo: Repository, project: str = "", top_n: int = 20) -> dict:
    """CI-20: a unified, prioritized report — every clone cluster,
    anti-pattern, and drift finding in one place, plus the CI-19
    aggregate scores. Prioritization is simple (severity DESC, then
    cluster/finding size DESC) — a real impact-weighted ranking would
    need CI-08's hot-path data joined in per finding, left as a documented
    follow-up rather than guessed at here.
    """
    scores = compute(repo, project=project)
    clone_clusters = repo.analysis_nodes(NodeKind.CLONE_CLUSTER.value, project=project)
    antipatterns = repo.analysis_nodes(NodeKind.ANTI_PATTERN.value, project=project)
    drift_findings = repo.analysis_nodes(NodeKind.DRIFT_FINDING.value, project=project)

    severity_rank = {"critical": 0, "warning": 1, "info": 2}
    prioritized: list[dict] = []
    for c in clone_clusters:
        prioritized.append({
            "kind": "clone_cluster",
            "severity": "warning",
            "label": c.label,
            "detail": f"{c.properties.get('member_count', 0)} members; "
                      f"consolidate into {c.properties.get('consolidation_target', '?')}",
        })
    for a in antipatterns:
        prioritized.append({
            "kind": "antipattern",
            "severity": a.properties.get("severity", "info"),
            "label": a.label,
            "detail": a.properties.get("detail", ""),
        })
    for f in drift_findings:
        prioritized.append({
            "kind": "drift_finding",
            "severity": f.properties.get("severity", "info"),
            "label": f.label,
            "detail": f.properties.get("detail", ""),
        })
    prioritized.sort(key=lambda p: severity_rank.get(p["severity"], 3))

    return {"scores": scores, "findings": prioritized[: max(1, int(top_n))]}
