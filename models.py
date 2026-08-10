"""Domain models for cie.

These dataclasses are the single source of truth for what a node, edge, and
query result look like. The repository layer translates Neo4j records into
these types; the query layer and CLI consume them. Nothing in this module
imports Neo4j, so the models can be used in tests without a database.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class NodeKind(str, Enum):
    """What a node represents in the knowledge graph.

    FILE  - a source file hub (e.g. ``client.py``).
    CLASS - a class or struct declaration.
    FUNC  - a module-level function.
    METHOD - a method declared inside a class.
    SYMBOL - any other extracted concept (kept for forward compatibility;
        also used for DM-08's `external::` base-class stub nodes).

    The four below are section 13 (Code Intelligence) analysis-result
    kinds — never produced by `extract.py`, only by the on-demand
    `cie.clone_detect`/`cie.perf_analyze`/`cie.drift_detect`/`cie.metrics`
    passes, written via `Neo4jRepository.replace_analysis_nodes`/
    `record_metric_snapshot` rather than `load_extraction`/`reindex_file`.

    CLONE_CLUSTER  - a group of cloned symbols (CI-04).
    ANTI_PATTERN   - a detected performance anti-pattern (CI-07).
    DRIFT_FINDING  - a detected spec-vs-code or architectural drift (CI-10/11/12).
    METRIC_SNAPSHOT - an append-only aggregate quality measurement (CI-19).
    """

    FILE = "file"
    CLASS = "class"
    FUNC = "function"
    METHOD = "method"
    SYMBOL = "symbol"
    CLONE_CLUSTER = "CloneCluster"
    ANTI_PATTERN = "AntiPattern"
    DRIFT_FINDING = "DriftFinding"
    METRIC_SNAPSHOT = "MetricSnapshot"
    #: RQ-04/AI-03: one LLM-generated thematic summary per detected
    #: community (`cie.community_detect`) — carries an `embedding` (unlike
    #: the four kinds above) so `community_search` can rank it via the
    #: same vector-index machinery `semantic_search`/`hybrid_search` use.
    COMMUNITY_SUMMARY = "CommunitySummary"
    #: Section 1 (Core Data Model) analysis-result kinds — same "never
    #: produced by extract.py, only by an on-demand `cie.data_model` pass,
    #: written via `replace_analysis_nodes`" contract as the four
    #: section-13 kinds above.
    #:
    #: TYPE - a resolved or synthesized stub node for a function/method's
    #:     parameter/return type (DM-09).
    #: PACKAGE - an external dependency parsed from a manifest file
    #:     (pyproject.toml/package.json) (DM-11).
    #: DOCUMENT - a markdown documentation file, chunked by heading
    #:     (DM-13).
    TYPE = "Type"
    PACKAGE = "Package"
    DOCUMENT = "Document"


class Confidence(str, Enum):
    """How an edge was derived from the source corpus.

    Mirrors the original cie confidence labels:
    - EXTRACTED: explicitly stated in the source (import, direct call).
    - INFERRED: a reasonable deduction (call-graph second pass, co-occurrence).
    - AMBIGUOUS: uncertain; flagged for human review.
    """

    EXTRACTED = "EXTRACTED"
    INFERRED = "INFERRED"
    AMBIGUOUS = "AMBIGUOUS"


class TraversalMode(str, Enum):
    """Graph traversal strategy exposed by the search command."""

    BFS = "bfs"
    DFS = "dfs"


@dataclass(frozen=True)
class Node:
    """A concept, file, or symbol extracted from the corpus."""

    id: str
    label: str
    source_file: str = ""
    source_location: str = ""
    file_type: str = ""
    community: Optional[int] = None
    kind: str = NodeKind.SYMBOL.value
    signature: str = ""
    line_start: int = 0
    line_end: int = 0
    docstring: str = ""
    project: str = ""
    embedding: tuple[float, ...] = field(default_factory=tuple)
    # IN-08 provenance/lineage — stamped at write time by
    # Neo4jRepository.load_extraction/.reindex_file (extract.py stays pure,
    # never sets these itself). "" is the "never stamped" default, not a
    # sentinel for a real value — old rows written before IN-08 shipped
    # simply read back with these empty rather than raising.
    extracted_at: str = ""
    extractor_version: str = ""
    source_ref: str = ""
    # Section 13 (Code Intelligence) analysis nodes carry properties that
    # don't fit any named field above (CloneCluster's `member_count`/
    # `consolidation_target`, AntiPattern's `severity`/`pattern`/
    # `suggested_fix`, DriftFinding's `drift_type`/`detail`) — rather than
    # keep bolting named fields onto EVERY Node for a handful of kinds
    # that need them, unmapped Neo4j properties land here. Every OTHER
    # node kind's `properties` stays `{}`; this is additive, not a
    # replacement for the named fields above (which stay the fast, typed
    # path for the properties every node kind actually shares).
    properties: dict = field(default_factory=dict)


@dataclass(frozen=True)
class CallSite:
    """One recorded call expression inside a source file (pass-2 input)."""

    caller_id: str
    called_name: str
    receiver: str = ""
    line: int = 0


@dataclass(frozen=True)
class ImportRecord:
    """One import binding in a source file (pass-2 import-map input)."""

    imported_name: str
    module: str
    kind: str  # "import" | "from_import" | "require"


@dataclass(frozen=True)
class SymbolMatch:
    """A search_symbols result: a node plus its score and confidence."""

    node: Node
    score: float
    confidence: Confidence


@dataclass(frozen=True)
class SemanticMatch:
    """A semantic_search result: a node plus its cosine-similarity score.

    `score` is the cosine similarity between the query embedding and the
    node's stored embedding — 0..1 for normalized embeddings in practice,
    but the field itself does not assume normalization.
    """

    node: Node
    score: float


@dataclass(frozen=True)
class HybridMatch:
    """A hybrid_search (RQ-01) result: one node plus its combined score
    AND the three component signals that produced it — lexical
    (fulltext-index label match), dense (vector-embedding similarity),
    graph (degree-centrality boost). Each component is already
    normalized to 0..1 before the weighted sum that produces `score` (see
    Neo4jRepository.hybrid_search for the exact weights/reasoning), so a
    caller can see WHY a result ranked where it did, not just the final
    number — useful for AI-01's citation trail (item 6), not just display."""

    node: Node
    score: float
    lexical_score: float
    dense_score: float
    graph_score: float


@dataclass(frozen=True)
class ContextHit:
    """A failing_context result: one symbol reachable from a test node."""

    node: Node
    distance: int
    confidence: Confidence


@dataclass(frozen=True)
class Edge:
    """A directed relationship between two nodes.

    `_source`/`_target` preserve original direction even when stored in an
    undirected representation, matching the original cie build step.
    """

    source: str
    target: str
    relation: str = ""
    confidence: Confidence = Confidence.EXTRACTED
    # IN-08 provenance/lineage — see Node's own fields for why these are
    # stamped at write time, not extraction time, and default to "".
    extracted_at: str = ""
    extractor_version: str = ""
    source_ref: str = ""
    # Section 13 analysis edges carry properties beyond the named fields
    # above (MEMBER_OF's `similarity`/`detected_by`) — same catch-all
    # rationale as Node.properties.
    properties: dict = field(default_factory=dict)


@dataclass(frozen=True)
class NodeRecord:
    """A node plus its computed degree, returned by node lookups."""

    node: Node
    degree: int


@dataclass(frozen=True)
class EdgeRecord:
    """An edge plus the labels of its endpoints, returned by neighbor lookups."""

    edge: Edge
    source_label: str
    target_label: str


@dataclass(frozen=True)
class TraversalResult:
    """Result of a BFS/DFS search from a set of start nodes."""

    mode: TraversalMode
    depth: int
    start_labels: tuple[str, ...]
    nodes: tuple[Node, ...]
    edges: tuple[EdgeRecord, ...]


@dataclass(frozen=True)
class PathResult:
    """A shortest path between two nodes, as an ordered list of hops."""

    hops: int
    nodes: tuple[Node, ...]
    edges: tuple[EdgeRecord, ...]


@dataclass(frozen=True)
class MethodSignature:
    """A function/method with its signature, returned by signature lookups."""

    node: Node
    name: str
    parameters: tuple[str, ...]
    return_type: str = ""
    enclosing_class: str = ""


@dataclass(frozen=True)
class GraphStats:
    """Aggregate statistics over the whole graph."""

    nodes: int
    edges: int
    communities: int
    confidence_counts: dict[str, int] = field(default_factory=dict)

    @property
    def confidence_percent(self) -> dict[str, float]:
        total = self.edges or 1
        return {k: round(v / total * 100) for k, v in self.confidence_counts.items()}


@dataclass(frozen=True)
class HealthStatus:
    """Lightweight repository health check result."""

    ok: bool
    nodes: int
    edges: int


@dataclass(frozen=True)
class FunctionCoverage:
    """One function/method's derived coverage within a covered file.

    Derived, not measured: computed by intersecting the coverage tool's
    reported uncovered-line set against this symbol's `line_start`/
    `line_end` range (already known from AST extraction) — see
    Neo4jRepository.record_coverage.
    """

    label: str
    line_start: int
    line_end: int
    coverage_pct: float


@dataclass(frozen=True)
class FileCoverage:
    """One file's coverage measurement plus its per-function breakdown."""

    file_path: str
    coverage_pct: float
    covered_lines: int
    uncovered_line_count: int
    uncovered_lines: tuple[int, ...]
    measured_at: str
    subtree: str = ""
    functions: tuple[FunctionCoverage, ...] = field(default_factory=tuple)


@dataclass(frozen=True)
class FileCoverageSummary:
    """One file's coverage, without the per-function detail — the shape
    `coverage_report` returns (a list of these, not full FileCoverage,
    keeps the report payload small). `measured` distinguishes "never had
    a coverage run" (False, `coverage_pct` is None) from "measured and
    genuinely at 0%" (True, `coverage_pct == 0.0`) — conflating the two
    would make "which files need better coverage" silently omit files
    nobody's ever run coverage on, the worst case, not a good one."""

    file_path: str
    measured: bool
    coverage_pct: Optional[float]
    uncovered_line_count: int
    measured_at: str
    subtree: str = ""


@dataclass(frozen=True)
class CoverageSnapshot:
    """One point-in-time aggregate coverage measurement for a project
    (optionally scoped to a `subtree`, e.g. "be-v2" vs "frontend" —
    Python and TS coverage numbers aren't meaningfully comparable, so
    trend/aggregate queries scope by subtree, not just project). One of
    these is written per coverage run (append-only — this is what makes
    "is coverage improving or regressing" answerable from history, not
    just the latest measurement)."""

    project: str
    subtree: str
    aggregate_pct: float
    files_measured: int
    total_lines: int
    covered_lines: int
    measured_at: str


@dataclass(frozen=True)
class MetricSnapshot:
    """One CI-19 aggregate quality metric measurement, append-only (same
    shape/rationale as `CoverageSnapshot` above — "is tech debt trending
    up or down" is only answerable from history, not a single reading).
    `metric_type` is one of "clone_coverage_pct" / "drift_index" /
    "tech_debt_score" (CI-05/CI-19's own vocabulary — no separate
    "refactor_score", cut for scope, see `cie.metrics` module docstring).
    """

    project: str
    metric_type: str
    value: float
    measured_at: str
    detail: dict = field(default_factory=dict)
