"""QG-01/02/03/04: graph quality/governance REPORTS.

All four functions here are read-mostly (QG-01 re-parses source files
from disk to compare against the graph, but writes nothing back) and
compose entirely from ALREADY-EXISTING `Repository` methods
(`code_symbol_nodes`, `list_files`, `project_graph`, `god_nodes`) — no
new persistence-layer methods needed, unlike section 13's analysis
passes.

QG-05 (RBAC/ABAC at query time) and QG-06 (immutable audit log) are
DELIBERATELY not attempted here — both are real, separate subsystems
(the former touches this project's existing RBAC policy system and is
security-sensitive; the latter needs a tamper-proof append-only log
design) that need their own scoping conversation, not silent inclusion
in a reporting-module pass. QG-08 (schema/distribution/quality drift)
is cut for scope: it overlaps thematically with section 13's
`cie.drift_detect` (code-vs-spec/architecture drift) without being the
same thing (QG-08 is about the GRAPH's own health over time, not code
correctness) — a real but lower-priority follow-up.
"""
from __future__ import annotations

import random
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from cie import extract
from cie.models import NodeKind
from cie.repository import Repository

_ANALYSIS_KINDS = {
    NodeKind.CLONE_CLUSTER.value, NodeKind.ANTI_PATTERN.value,
    NodeKind.DRIFT_FINDING.value, NodeKind.METRIC_SNAPSHOT.value,
    NodeKind.COMMUNITY_SUMMARY.value,
}

_EXCLUDED_DIR_PARTS = {"node_modules", "__pycache__", ".venv"}


def accuracy_check(repo: Repository, sample_size: int = 20, seed: Optional[int] = None) -> dict:
    """QG-01: re-parse a random sample of already-indexed FUNC/METHOD
    symbols straight from disk and compare `signature`/`docstring`/
    `line_start` against what the graph currently stores. A mismatch
    means the graph is STALE for that symbol (the file changed since
    the last load/reindex) — this is a point-in-time accuracy check, not
    a fix; a caller finding mismatches should re-run `reindex`.

    `seed` makes the sample reproducible for tests; `None` (the
    default) uses a fresh random sample each call, which is what a real
    periodic accuracy-sampling job wants (different files checked each
    run, not the same ones forever).
    """
    candidates = repo.code_symbol_nodes()
    if not candidates:
        return {"sampled": 0, "mismatches": 0, "mismatch_rate": 0.0, "detail": []}

    rng = random.Random(seed)
    sample = rng.sample(candidates, min(sample_size, len(candidates)))

    # Re-parse each sampled node's FILE once, not once per symbol.
    by_file: dict[str, list] = {}
    for node in sample:
        by_file.setdefault(node.source_file, []).append(node)

    mismatches: list[dict] = []
    checked = 0
    for source_file, nodes in by_file.items():
        try:
            fresh = extract.extract_file(Path(source_file))
        except ValueError:
            # File deleted/unreadable since it was indexed — every
            # symbol from it is definitionally stale, not skipped.
            for node in nodes:
                checked += 1
                mismatches.append({
                    "id": node.id, "label": node.label,
                    "reason": "source file no longer readable",
                })
            continue
        fresh_by_id = {n["id"]: n for n in fresh.nodes}
        for node in nodes:
            checked += 1
            current = fresh_by_id.get(node.id)
            if current is None:
                mismatches.append({
                    "id": node.id, "label": node.label,
                    "reason": "symbol no longer found at this id (renamed, "
                              "moved, or removed)",
                })
                continue
            changed = [
                f for f in ("signature", "docstring", "line_start")
                if str(current.get(f, "")) != str(getattr(node, f, ""))
            ]
            if changed:
                mismatches.append({
                    "id": node.id, "label": node.label,
                    "reason": f"changed fields: {', '.join(changed)}",
                })

    rate = round(len(mismatches) / checked, 4) if checked else 0.0
    return {"sampled": checked, "mismatches": len(mismatches), "mismatch_rate": rate, "detail": mismatches}


def freshness_report(repo: Repository, stale_after_days: float = 7.0, project: str = "") -> dict:
    """QG-02: age every node's `extracted_at` (IN-08 provenance —
    already stamped at write time, no new tracking needed) against
    `stale_after_days`. Nodes with no `extracted_at` at all (written
    before IN-08 shipped, or a section-13 analysis kind that doesn't
    always stamp one) are reported SEPARATELY as `never_stamped`, not
    folded into `stale` — "unknown age" and "confirmed old" are
    different findings and conflating them would hide which one a
    caller is actually looking at.
    """
    nodes, _edges = repo.project_graph()
    now = datetime.now(timezone.utc)
    stale: list[str] = []
    never_stamped: list[str] = []
    ages_days: list[float] = []

    for node in nodes:
        if not node.extracted_at:
            never_stamped.append(node.id)
            continue
        try:
            stamped = datetime.fromisoformat(node.extracted_at)
        except ValueError:
            never_stamped.append(node.id)
            continue
        age_days = (now - stamped).total_seconds() / 86400
        ages_days.append(age_days)
        if age_days > stale_after_days:
            stale.append(node.id)

    total = len(nodes)
    return {
        "total_nodes": total,
        "stale_count": len(stale),
        "stale_pct": round(len(stale) / total * 100, 2) if total else 0.0,
        "never_stamped_count": len(never_stamped),
        "oldest_age_days": round(max(ages_days), 2) if ages_days else None,
        "newest_age_days": round(min(ages_days), 2) if ages_days else None,
        "stale_after_days": stale_after_days,
    }


def comprehensiveness_report(repo: Repository, repo_root: Optional[Path] = None) -> dict:
    """QG-03: three coverage ratios in one report —

    - `file_index_pct`: FILE nodes in the graph / supported source files
      actually on disk under `repo_root` (skipped entirely, `None`, when
      `repo_root` isn't supplied — this ratio needs filesystem access
      the other two don't).
    - `docstring_pct`: FUNC/METHOD symbols with a non-empty docstring.
    - `call_resolution_pct`: `calls` edges resolved at EXTRACTED
      confidence (same-file/import-map certain) vs INFERRED/AMBIGUOUS
      (heuristic-resolved) — see `cie.callgraph`'s own confidence
      tiers.
    """
    symbols = repo.code_symbol_nodes()
    with_docstring = sum(1 for n in symbols if n.docstring)
    docstring_pct = round(with_docstring / len(symbols) * 100, 2) if symbols else 0.0

    _nodes, edge_records = repo.project_graph()
    call_edges = [er for er in edge_records if er.edge.relation == "calls"]
    confidence_counts = Counter(er.edge.confidence.value for er in call_edges)
    extracted = confidence_counts.get("EXTRACTED", 0)
    call_resolution_pct = (
        round(extracted / len(call_edges) * 100, 2) if call_edges else 0.0
    )

    file_index_pct: Optional[float] = None
    files_on_disk: Optional[int] = None
    if repo_root is not None:
        files_on_disk = sum(
            1 for p in Path(repo_root).rglob("*")
            if p.is_file() and extract.supported_suffix(p) is not None
            and not any(part.startswith(".") or part in _EXCLUDED_DIR_PARTS for part in p.parts)
        )
        files_indexed = len(repo.list_files())
        file_index_pct = (
            round(min(files_indexed, files_on_disk) / files_on_disk * 100, 2)
            if files_on_disk else 0.0
        )

    return {
        "file_index_pct": file_index_pct,
        "files_on_disk": files_on_disk,
        "docstring_pct": docstring_pct,
        "symbols_checked": len(symbols),
        "call_resolution_pct": call_resolution_pct,
        "call_confidence_breakdown": dict(confidence_counts),
    }


def salience_report(repo: Repository, top_n: int = 20) -> list[dict]:
    """QG-04, READ-ONLY — a report of the LEAST salient symbols (pruning
    CANDIDATES, ranked lowest-salience-first), never an actual deletion.
    Pruning is destructive; this module only ever reports.

    `salience = degree + docstring_bonus + community_bonus`: `degree`
    (callers+callees+contains/defines edges) is the dominant term — an
    unconnected leaf function is the textbook "safe to remove without
    losing a critical path" case the spec describes; a short bonus for
    having a docstring (documented code is more likely intentionally
    kept) and for being in a decent-sized community (isolated code is
    more prunable than something embedded in a larger, connected
    cluster) nudge the ranking, not dominate it.
    """
    symbols = repo.code_symbol_nodes()
    if not symbols:
        return []
    degree_by_id: dict[str, int] = {}
    for nr in repo.god_nodes(top_n=len(symbols)):
        degree_by_id[nr.node.id] = nr.degree
    community_sizes = dict(repo.list_communities())

    scored: list[tuple[float, object]] = []
    for n in symbols:
        degree = degree_by_id.get(n.id, 0)
        docstring_bonus = 1.0 if n.docstring else 0.0
        community_bonus = min(2.0, (community_sizes.get(n.community, 0) or 0) / 10.0) if n.community is not None else 0.0
        salience = degree + docstring_bonus + community_bonus
        scored.append((salience, n))

    scored.sort(key=lambda pair: pair[0])
    return [
        {
            "id": n.id, "label": n.label, "source_file": n.source_file,
            "salience": round(score, 2), "degree": degree_by_id.get(n.id, 0),
            "has_docstring": bool(n.docstring),
        }
        for score, n in scored[:top_n]
    ]
