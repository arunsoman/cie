"""Section 17: System Intelligence & Subsystem Health (spec section 17).

SI-01's registry describes every subsystem THIS CODEBASE actually has
after sections 0/1/13/14/15 were built — not a hypothetical/exhaustive
list of all ~180 spec requirements, most of which were never given a
distinct `NodeKind` to count. It is a static, hand-maintained Python
structure (per the spec's own "not in Neo4j — it's meta-metadata about
the spec itself" framing).

Deviation from the spec text: `population_query` is a Python CALLABLE
`(repo, project) -> int`, not a raw Cypher string. Every module in this
codebase is tested against BOTH `Neo4jRepository` and `InMemoryRepository`
(see `tests/in_memory_repo.py`) — a raw Cypher string would only work
against the real backend, breaking that test-portability. A callable
built from the SAME `Repository` protocol methods (`analysis_nodes`,
`project_graph`, `list_communities`) every other module already calls
works against both.

Two registered subsystems (`confidence`, `justification`) compute their
output ON DEMAND and never persist a `NodeKind` of their own — their
`population_query` always returns 0 with `min_expected_nodes=0`, which
`check_population` treats as trivially "populated" (there's no state of
THEIRS to be empty). Their real readiness is a function of their
`consumes` list, which `downstream_impact`/`missing_data_report` already
account for — this is a deliberate modeling choice, not an oversight.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any, Callable, Optional, Sequence


@dataclass(frozen=True)
class Subsystem:
    subsystem_id: str
    name: str
    section: str
    population_query: Callable[[Any, str], int]
    produces_nodes: tuple[str, ...] = ()
    produces_edges: tuple[str, ...] = ()
    consumes: tuple[str, ...] = ()
    feeds: tuple[str, ...] = ()
    min_expected_nodes: int = 1
    #: SI-02's "stale" status needs a per-subsystem freshness signal —
    #: `(repo, project) -> Optional[str]` returning the MOST RECENT
    #: IN-08 `extracted_at` timestamp among this subsystem's own nodes/
    #: edges, or `None` when no timestamp source exists (community
    #: detection has neither a node nor edge of its own to stamp; the
    #: two on-demand subsystems have no persisted state at all). `None`
    #: means "staleness is not evaluable for this subsystem" — NOT "so
    #: treat it as stale," which would be actively misleading for a
    #: subsystem that's actually fine, just unmeasurable this way.
    freshness_query: Optional[Callable[[Any, str], Optional[str]]] = None


def _count_kind(kind: str) -> Callable[[Any, str], int]:
    def _fn(repo: Any, project: str) -> int:
        return len(repo.analysis_nodes(kind, project=project))
    return _fn


def _count_edges(relation: str) -> Callable[[Any, str], int]:
    def _fn(repo: Any, project: str) -> int:
        _nodes, edges = repo.project_graph()
        return sum(1 for er in edges if er.edge.relation == relation)
    return _fn


def _count_communities() -> Callable[[Any, str], int]:
    def _fn(repo: Any, _project: str) -> int:
        return len(repo.list_communities())
    return _fn


def _zero(_repo: Any, _project: str) -> int:
    return 0


def _freshness_kind(kind: str) -> Callable[[Any, str], Optional[str]]:
    """Most recent `extracted_at` among a kind's own nodes, or `None`
    when the kind has no nodes, or none of them were ever stamped
    (pre-IN-08 rows, or a kind this codebase writes without provenance —
    `""` sorts first, so a mix of stamped/unstamped rows still picks the
    real most-recent stamped one correctly via a max() over non-empty
    values only)."""
    def _fn(repo: Any, project: str) -> Optional[str]:
        timestamps = [n.extracted_at for n in repo.analysis_nodes(kind, project=project) if n.extracted_at]
        return max(timestamps) if timestamps else None
    return _fn


def _freshness_edges(relation: str) -> Callable[[Any, str], Optional[str]]:
    """Most recent `extracted_at` among edges of one relation, or `None`."""
    def _fn(repo: Any, _project: str) -> Optional[str]:
        _nodes, edges = repo.project_graph()
        timestamps = [
            er.edge.extracted_at for er in edges
            if er.edge.relation == relation and er.edge.extracted_at
        ]
        return max(timestamps) if timestamps else None
    return _fn


REGISTRY: tuple[Subsystem, ...] = (
    Subsystem("clone_detect", "Clone Detector", "13.1",
              population_query=_count_kind("CloneCluster"), produces_nodes=("CloneCluster",),
              feeds=("metrics",), freshness_query=_freshness_kind("CloneCluster")),
    Subsystem("perf_analyze", "Performance Analyzer", "13.2",
              population_query=_count_kind("AntiPattern"), produces_nodes=("AntiPattern",),
              feeds=("metrics",), freshness_query=_freshness_kind("AntiPattern")),
    Subsystem("drift_detect", "Drift Detector", "13.3",
              population_query=_count_kind("DriftFinding"), produces_nodes=("DriftFinding",),
              feeds=("metrics",), freshness_query=_freshness_kind("DriftFinding")),
    Subsystem("metrics", "Metric Computer", "13.5",
              population_query=_count_kind("MetricSnapshot"), produces_nodes=("MetricSnapshot",),
              consumes=("clone_detect", "perf_analyze", "drift_detect"),
              freshness_query=_freshness_kind("MetricSnapshot")),
    Subsystem("community_detect", "Community Detection", "3 (RQ-04)",
              population_query=_count_communities(), feeds=("community_summarize",)),
    Subsystem("community_summarize", "Community Summarization", "3 (AI-03)",
              population_query=_count_kind("CommunitySummary"), produces_nodes=("CommunitySummary",),
              consumes=("community_detect",), freshness_query=_freshness_kind("CommunitySummary")),
    Subsystem("inheritance", "Inheritance/Interface Edges", "1 (DM-08)",
              population_query=_count_edges("extends"), produces_edges=("extends", "implements"),
              feeds=("traceability",), freshness_query=_freshness_edges("extends")),
    Subsystem("testlink", "Test-to-Implementation Edges", "1 (DM-14)",
              population_query=_count_edges("TESTS"), produces_edges=("TESTS",),
              feeds=("traceability",), freshness_query=_freshness_edges("TESTS")),
    Subsystem("type_flow", "Type/Data-Flow Edges", "1 (DM-09)",
              population_query=_count_kind("Type"), produces_nodes=("Type",), produces_edges=("TYPE_OF",),
              freshness_query=_freshness_edges("TYPE_OF")),
    Subsystem("dependency_graph", "External Dependency Graph", "1 (DM-11)",
              population_query=_count_kind("Package"), produces_nodes=("Package",),
              freshness_query=_freshness_kind("Package")),
    Subsystem("doc_graph", "Documentation Graph", "1 (DM-13)",
              population_query=_count_kind("Document"), produces_nodes=("Document",),
              produces_edges=("DESCRIBES",), freshness_query=_freshness_kind("Document")),
    Subsystem("contracts", "ContractExtractor", "14.1 (CF-01)",
              population_query=_count_kind("Contract"), produces_nodes=("Contract",),
              produces_edges=("HAS_CONTRACT",), feeds=("traceability", "invariants"),
              freshness_query=_freshness_kind("Contract")),
    Subsystem("test_synthesis", "AcceptanceTestGenerator", "14.2 (CF-04)",
              population_query=_count_kind("TestSkeleton"), produces_nodes=("TestSkeleton",),
              feeds=("traceability",), freshness_query=_freshness_kind("TestSkeleton")),
    Subsystem("state_machine", "StateMachineBuilder", "14.3 (CF-06)",
              population_query=_count_kind("StateMachine"),
              produces_nodes=("StateMachine", "State", "Transition"),
              freshness_query=_freshness_kind("StateMachine")),
    Subsystem("traceability", "TraceabilityMatrixEngine", "14.4 (CF-08)",
              population_query=_count_edges("TESTS"),
              consumes=("contracts", "test_synthesis", "inheritance", "testlink"),
              feeds=("confidence", "justification"), freshness_query=_freshness_edges("TESTS")),
    Subsystem("consensus", "Multi-Agent Consensus (verdicts)", "14.6 (CF-12)",
              population_query=_count_kind("AgentVerdict"), produces_nodes=("AgentVerdict",),
              feeds=("confidence",), freshness_query=_freshness_kind("AgentVerdict")),
    Subsystem("confidence", "ConfidenceScoreCalculator", "14.7 (CF-15)",
              population_query=_zero, consumes=("traceability", "consensus"),
              feeds=("justification",), min_expected_nodes=0),
    Subsystem("justification", "JustificationTraceEmitter", "14.8 (CF-17)",
              population_query=_zero, consumes=("traceability", "confidence"), min_expected_nodes=0),
    Subsystem("invariants", "InvariantMonitor", "14.9 (CF-19)",
              population_query=_count_kind("InvariantViolation"), produces_nodes=("InvariantViolation",),
              consumes=("contracts",), freshness_query=_freshness_kind("InvariantViolation")),
    Subsystem("decompose_pages", "Page Extractor", "15.1 (DE-01)",
              population_query=_count_kind("Page"), produces_nodes=("Page",),
              feeds=("decompose_elements", "implied_pages"), freshness_query=_freshness_kind("Page")),
    Subsystem("decompose_elements", "Interactive Element Detector", "15.1 (DE-02)",
              population_query=_count_kind("InteractiveElement"), produces_nodes=("InteractiveElement",),
              consumes=("decompose_pages",), feeds=("decompose_tasks",),
              freshness_query=_freshness_kind("InteractiveElement")),
    Subsystem("decompose_tasks", "Element-to-Task Decomposition", "15.1 (DE-04)",
              population_query=_count_kind("DerivedTaskHint"), produces_nodes=("DerivedTaskHint",),
              consumes=("decompose_elements",), freshness_query=_freshness_kind("DerivedTaskHint")),
    Subsystem("implied_pages", "Implied Navigation Tree", "15.1 (DE-03)",
              population_query=_count_kind("ImpliedPage"), produces_nodes=("ImpliedPage",),
              consumes=("decompose_pages",), freshness_query=_freshness_kind("ImpliedPage")),
)

BY_ID: dict[str, Subsystem] = {s.subsystem_id: s for s in REGISTRY}


# -- SI-02: Subsystem Population Check ---------------------------------------

#: SI-02's default TTL: a POPULATED subsystem whose most recent
#: `extracted_at` is older than this is "stale" — data exists but hasn't
#: been refreshed in a while, distinct from never having been populated
#: at all. 30 days is a deliberately loose default (this is a code
#: knowledge graph, not a live metrics feed — most subsystems are
#: expected to go a while between re-extraction runs); callers scan a
#: fast-changing project can pass a tighter value.
DEFAULT_STALE_AFTER_DAYS = 30.0


@dataclass(frozen=True)
class SubsystemStatus:
    subsystem_id: str
    status: str  # "populated" | "partial" | "empty" | "stale"
    count: int
    checked_at: str
    last_updated: Optional[str] = None


def check_population(
    repo: Any, project: str, subsystem: Subsystem,
    stale_after_days: float = DEFAULT_STALE_AFTER_DAYS, now: Optional[datetime] = None,
) -> SubsystemStatus:
    """SI-02: run one subsystem's `population_query` and classify it.
    `"stale"` is only ever considered for a subsystem that would
    otherwise be `"populated"` (an empty/partial subsystem is already a
    more specific, more actionable finding than "old") AND that has a
    `freshness_query` at all — see `Subsystem.freshness_query`'s own
    docstring for why `None` means "not evaluable," not "assume stale."
    """
    count = subsystem.population_query(repo, project)
    if count == 0:
        status = "populated" if subsystem.min_expected_nodes == 0 else "empty"
    elif count < subsystem.min_expected_nodes:
        status = "partial"
    else:
        status = "populated"

    last_updated: Optional[str] = None
    if status == "populated" and subsystem.freshness_query is not None:
        last_updated = subsystem.freshness_query(repo, project)
        if last_updated:
            now = now or datetime.now(timezone.utc)
            try:
                age = now - datetime.fromisoformat(last_updated)
            except ValueError:
                age = None
            if age is not None and age > timedelta(days=stale_after_days):
                status = "stale"

    return SubsystemStatus(
        subsystem.subsystem_id, status, count, datetime.now(timezone.utc).isoformat(), last_updated,
    )


def subsystem_health_report(
    repo: Any, project: str, registry: Sequence[Subsystem] = REGISTRY,
    stale_after_days: float = DEFAULT_STALE_AFTER_DAYS,
) -> dict:
    """SI-02/SI-08: one status per registered subsystem, plus a summary
    count. This IS the health dashboard's data (SI-08 adds only a color
    annotation on top, see `health_dashboard`)."""
    rows = []
    for s in registry:
        st = check_population(repo, project, s, stale_after_days=stale_after_days)
        rows.append({
            "subsystem_id": s.subsystem_id, "name": s.name, "section": s.section,
            "status": st.status, "count": st.count, "checked_at": st.checked_at,
            "last_updated": st.last_updated,
        })
    return {
        "project": project,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "subsystems": rows,
        "summary": {
            "populated": sum(1 for r in rows if r["status"] == "populated"),
            "partial": sum(1 for r in rows if r["status"] == "partial"),
            "empty": sum(1 for r in rows if r["status"] == "empty"),
            "stale": sum(1 for r in rows if r["status"] == "stale"),
            "total": len(rows),
        },
    }


# -- SI-03: Downstream Impact Analysis ---------------------------------------

def downstream_impact(
    subsystem_id: str, statuses: dict[str, str], registry: Sequence[Subsystem] = REGISTRY,
) -> list[dict]:
    """SI-03: from one empty/partial subsystem, walk `feeds` to find
    every downstream subsystem. `"blocked"` when the downstream
    subsystem's OWN status is also empty/partial (nothing of its own
    exists without this input); `"degraded"` when it's populated anyway
    (it has SOME data, but quality may be reduced by the missing
    upstream — a judgment call this function surfaces, doesn't resolve).
    """
    by_id = {s.subsystem_id: s for s in registry}
    root = by_id.get(subsystem_id)
    if root is None:
        return []
    impact: list[dict] = []
    frontier = list(root.feeds)
    seen: set[str] = set()
    while frontier:
        sid = frontier.pop(0)
        if sid in seen or sid not in by_id:
            continue
        seen.add(sid)
        downstream = by_id[sid]
        status = statuses.get(sid, "empty")
        impact.append({
            "subsystem_id": sid, "name": downstream.name,
            "impact": "blocked" if status in ("empty", "partial") else "degraded",
        })
        frontier.extend(downstream.feeds)
    return impact


# -- SI-04: Missing Data Report -----------------------------------------------

def missing_data_report(report: dict, registry: Sequence[Subsystem] = REGISTRY) -> list[dict]:
    """SI-04: for every empty/partial subsystem in an already-computed
    `subsystem_health_report`, explain WHY (an unmet upstream dependency,
    or simply never run) and WHAT TO DO."""
    by_id = {s.subsystem_id: s for s in registry}
    statuses = {r["subsystem_id"]: r["status"] for r in report["subsystems"]}
    out: list[dict] = []
    for row in report["subsystems"]:
        if row["status"] not in ("empty", "partial"):
            continue
        subsystem = by_id.get(row["subsystem_id"])
        if subsystem is None:
            continue
        unmet = [c for c in subsystem.consumes if statuses.get(c) in ("empty", "partial")]
        if unmet:
            reason = f"upstream dependency not populated: {', '.join(unmet)}"
            action = f"populate {unmet[0]} first"
        else:
            reason = "extraction/analysis tool not yet run for this project"
            action = f"run the {subsystem.subsystem_id} tool"
        out.append({
            "subsystem_id": subsystem.subsystem_id, "name": subsystem.name,
            "status": row["status"], "reason": reason, "recommended_action": action,
        })
    return out


# -- SI-05: Confidence Impact Notification -----------------------------------

#: CF-15's `layer_scores` keys -> the subsystem(s) that back them. `[]`
#: means "no subsystem backs this layer at all" (CF-15's `generation`/
#: `runtime` layers — always None regardless of subsystem health).
_CONFIDENCE_LAYER_SUBSYSTEMS: dict[str, tuple[str, ...]] = {
    "spec": ("contracts",),
    "testing": ("test_synthesis", "testlink"),
    "consensus": ("consensus",),
    "generation": (),
    "runtime": (),
}


def annotate_confidence_data_sources(confidence_report: dict, health_report: dict) -> dict:
    """SI-05: adds `layer_data_sources` to a CF-15 `confidence_report` —
    per layer, whether the subsystem(s) backing it are populated/
    partial/empty/not_applicable. CF-15's `layer_scores` already reads
    `None` (not `0.0`) for an unmeasured layer; this adds WHY, so a
    caller can render "Contract layer: N/A (ContractExtractor empty)"
    instead of a bare `None`."""
    statuses = {r["subsystem_id"]: r["status"] for r in health_report["subsystems"]}
    data_sources: dict[str, str] = {}
    for layer, subsystem_ids in _CONFIDENCE_LAYER_SUBSYSTEMS.items():
        if not subsystem_ids:
            data_sources[layer] = "not_applicable"
            continue
        layer_statuses = [statuses.get(sid, "empty") for sid in subsystem_ids]
        if all(s == "populated" for s in layer_statuses):
            data_sources[layer] = "populated"
        elif any(s in ("populated", "partial") for s in layer_statuses):
            data_sources[layer] = "partial"
        else:
            data_sources[layer] = "empty"
    return {**confidence_report, "layer_data_sources": data_sources}


# -- SI-06: Subsystem Dependency Graph (meta-graph) --------------------------

def dependency_graph_nodes(registry: Sequence[Subsystem] = REGISTRY) -> tuple[list[dict], list[dict]]:
    """SI-06: the registry itself AS a graph — `Subsystem` nodes +
    `SUBSYSTEM_DEPENDS_ON`/`SUBSYSTEM_FEEDS` edges. Meant for a `_meta`
    project namespace (just another project string, per this codebase's
    existing project-scoping mechanism — no special Neo4j feature needed
    for "a dedicated namespace")."""
    nodes = [
        {
            "id": f"subsystem::{s.subsystem_id}", "label": s.name, "kind": "Subsystem",
            "source_file": "", "subsystem_id": s.subsystem_id, "section": s.section,
        }
        for s in registry
    ]
    edges: list[dict] = []
    for s in registry:
        for dep in s.consumes:
            edges.append({
                "source": f"subsystem::{s.subsystem_id}", "target": f"subsystem::{dep}",
                "relation": "SUBSYSTEM_DEPENDS_ON", "confidence": "EXTRACTED",
            })
        for target in s.feeds:
            edges.append({
                "source": f"subsystem::{s.subsystem_id}", "target": f"subsystem::{target}",
                "relation": "SUBSYSTEM_FEEDS", "confidence": "EXTRACTED",
            })
    return nodes, edges


# -- SI-07: Population Path Finder -------------------------------------------

def population_path(capability_subsystem_id: str, registry: Sequence[Subsystem] = REGISTRY) -> list[str]:
    """SI-07: the minimum set of subsystems, in dependency order, that
    must be populated to enable `capability_subsystem_id` — backward
    traversal over `consumes`. `[]` for an unknown id."""
    by_id = {s.subsystem_id: s for s in registry}
    if capability_subsystem_id not in by_id:
        return []
    order: list[str] = []
    seen: set[str] = set()

    def _visit(sid: str) -> None:
        if sid in seen or sid not in by_id:
            return
        seen.add(sid)
        for dep in by_id[sid].consumes:
            _visit(dep)
        order.append(sid)

    _visit(capability_subsystem_id)
    return order


# -- SI-08: Health Dashboard --------------------------------------------------

_STATUS_COLOR = {"populated": "green", "partial": "yellow", "empty": "red", "stale": "gray"}


def health_dashboard(report: dict) -> dict:
    """SI-08: `subsystem_health_report`'s output, with a `color`
    (green/yellow/red/gray) added per subsystem — the whole payload a
    frontend dashboard needs, nothing computed twice."""
    return {
        **report,
        "subsystems": [
            {**row, "color": _STATUS_COLOR.get(row["status"], "gray")}
            for row in report["subsystems"]
        ],
    }


# -- SI-09: Population Alerting (transition detection, not a live scheduler) --

def detect_transitions(previous: dict, current: dict) -> list[dict]:
    """SI-09: compare two `subsystem_health_report` snapshots and flag
    subsystems that regressed from `populated` to `empty`/`stale`. NOT a
    live scheduler — no TE-09 telemetry distribution bus exists in this
    codebase to trigger this automatically on a real transition; a
    caller must supply both snapshots (e.g. from a periodic health-check
    cron calling `subsystem_health_report` and diffing against its own
    last-saved copy)."""
    prev_by_id = {r["subsystem_id"]: r["status"] for r in previous["subsystems"]}
    alerts = []
    for row in current["subsystems"]:
        prev_status = prev_by_id.get(row["subsystem_id"])
        if prev_status == "populated" and row["status"] in ("empty", "stale"):
            alerts.append({
                "subsystem_id": row["subsystem_id"],
                "from_status": prev_status, "to_status": row["status"],
            })
    return alerts


# -- SI-10: Boot-Time Subsystem Check -----------------------------------------

def boot_subsystem_check_summary(report: dict) -> str:
    """SI-10: one-line startup summary, e.g. "12 of 21 subsystems
    populated, 5 empty (contracts, test_synthesis, ...)." Deliberately
    NOT auto-wired into `factory.get_engine()` — that path is called
    (and cached) per project namespace, but `subsystem_health_report`
    runs ~21 real repository queries; adding that cost to every first
    engine construction for every project is a caller's choice to make
    explicitly (a CLI boot path, or an admin healthcheck endpoint), not
    something this module should impose silently on every process start.
    """
    empty = [r["subsystem_id"] for r in report["subsystems"] if r["status"] == "empty"]
    partial = [r["subsystem_id"] for r in report["subsystems"] if r["status"] == "partial"]
    summary = f"{report['summary']['populated']} of {report['summary']['total']} subsystems populated"
    if empty:
        preview = ", ".join(empty[:5]) + ("..." if len(empty) > 5 else "")
        summary += f", {len(empty)} empty ({preview})"
    if partial:
        preview = ", ".join(partial[:5]) + ("..." if len(partial) > 5 else "")
        summary += f", {len(partial)} partial ({preview})"
    return summary
