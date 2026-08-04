"""Repository protocol.

The query engine depends on this abstract interface, not on Neo4j directly.
That keeps the engine testable with an in-memory fake and lets us swap storage
backends without touching query logic.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Optional, Protocol, Sequence, runtime_checkable

if TYPE_CHECKING:
    from cie.extract import Extraction

from cie.models import (
    ContextHit,
    CoverageSnapshot,
    EdgeRecord,
    FileCoverage,
    FileCoverageSummary,
    GraphStats,
    MethodSignature,
    Node,
    NodeRecord,
    PathResult,
    SemanticMatch,
    SymbolMatch,
    TraversalMode,
    TraversalResult,
)


@runtime_checkable
class Repository(Protocol):
    """Persistence and graph-traversal contract for the querying engine."""

    def find_nodes_by_terms(self, terms: Sequence[str]) -> list[tuple[float, Node]]:
        """Score nodes against search terms; return (score, node) desc."""

    def get_node(self, label_or_id: str) -> Optional[NodeRecord]:
        """Return a node matching a label substring or exact id, with degree."""

    def get_neighbors(
        self, label_or_id: str, relation_filter: str = ""
    ) -> Optional[list[EdgeRecord]]:
        """Return a node's direct neighbors, or None if the node is unknown."""

    def get_community(self, community_id: int) -> list[Node]:
        """Return all nodes in a community, or an empty list if it is empty."""

    def list_communities(self) -> list[tuple[int, int]]:
        """Return (community_id, size) pairs ordered by size descending."""

    def god_nodes(self, top_n: int = 10) -> list[NodeRecord]:
        """Return the most-connected real entities (file hubs excluded)."""

    def stats(self) -> GraphStats:
        """Return whole-graph aggregate statistics."""

    def shortest_path(
        self, source: str, target: str, max_hops: int = 8
    ) -> Optional[PathResult]:
        """Find the shortest path between two concepts, or None if unreachable."""

    def traverse(
        self,
        start_labels: Sequence[str],
        mode: TraversalMode,
        depth: int,
    ) -> TraversalResult:
        """BFS or DFS traversal from the given start node labels."""

    # -- loader -----------------------------------------------------------

    def load_extraction(
        self,
        nodes: Sequence[dict],
        edges: Sequence[dict],
        project: str = "",
    ) -> int:
        """Replace the graph contents with the given nodes/edges.

        Returns the number of nodes written. Implementations must clear the
        existing graph before inserting so repeated loads are idempotent.
        With a non-empty `project`, only that project's slice is replaced
        and every node is stamped with the project namespace.
        """

    def reindex_file(
        self, path: str, extraction: "Extraction", project: str = ""
    ) -> int:
        """Incrementally re-index one file; read-after-write consistent.

        Deletes the file's symbol nodes and their edges, inserts the fresh
        extraction, and re-resolves pass-2 `calls` edges for the file and
        its importers. Returns the number of nodes written.
        """

    # -- code-structure queries -------------------------------------------

    def get_signature(self, name: str) -> Optional[MethodSignature]:
        """Return the signature of a function/method by name, or None."""

    def get_methods_of_class(self, class_name: str) -> list[MethodSignature]:
        """Return all methods declared in the named class."""

    def discover_features(self) -> list[str]:
        """Return a human/LLM-readable list of the querying features available."""

    def list_files(self) -> list[Node]:
        """Return all file-level nodes in the graph."""

    def project_graph(self) -> tuple[list[Node], list[EdgeRecord]]:
        """Every node in this project's code-structure graph, plus every
        RELATES edge between two of them — a whole-project dump, unlike
        every other query method here which scopes to a single symbol,
        file, or search term. Backs GET /api/projects/{id}/code-graph."""

    # -- forge tool-layer queries (Wave B) ----------------------------------

    def search_symbols(
        self, name: str, kind: str = "", file_glob: str = "", limit: int = 20
    ) -> list[SymbolMatch]:
        """Exact-name matches score 2.0, substring 1.0; kind in
        class|function|method|file ("" = all); file_glob fnmatch on source_file."""

    def get_callers(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Nodes with an outgoing RELATES{relation:'calls'} edge TO symbol.
        Empty list (not None) when symbol unknown — engine adds the hint."""

    def get_callees(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Reverse of get_callers."""

    def get_file_skeleton(self, path: str) -> list[Node]:
        """All class/function/method nodes in file (substring match on source_file),
        ordered by line_start. No bodies — signature/docstring/lines only."""

    def failing_context(
        self, test_identifier: str, depth: int = 3, limit: int = 30
    ) -> list[ContextHit]:
        """Locate the test node (label contains the ::-suffix function name, or the
        file for a bare path), BFS over outgoing calls/contains/defines edges,
        group by graph distance, nearest first."""

    def affected_by(
        self, file_path: str, max_depth: int = 3, direction: str = "incoming",
        max_results: int = 30,
    ) -> list[ContextHit]:
        """Blast radius (docs/forge-rebuild-plan.md's Phase 2 / WS4, ported
        from backend/src/cie/graph_interface.py's `affected_by`,
        direction flipped per review finding P1.8): BFS over `RELATES`
        edges up to `max_depth` hops FROM every node whose source_file
        matches `file_path`. direction="incoming" (the default — what
        depends on this file, i.e. what breaks if it changes) traverses
        edges pointing INTO the file's nodes; direction="outgoing" (what
        this file depends on) traverses edges pointing OUT of them.
        Grouped by graph distance, nearest first, capped at
        `max_results`. Empty list when the file has no indexed nodes."""

    def health_details(self) -> dict:
        """{store: 'reachable', nodes, edges, communities, project} or raise."""

    def semantic_search(
        self, query: str, top_k: int = 10, project: str = ""
    ) -> list[SemanticMatch]:
        """Rank nodes by cosine similarity between `query`'s embedding and
        each node's stored embedding. Empty list when nothing has an
        embedding yet. `project` optionally overrides the repository's own
        project scoping (empty = use whatever the repo was constructed
        with)."""

    # -- QA coverage (be-v2/docs/design/qa-persona-cie-knowledge-graph.md) --

    def record_coverage(
        self,
        file_path: str,
        subtree: str,
        coverage_pct: float,
        covered_lines: int,
        uncovered_lines: Sequence[int],
        measured_at: str,
    ) -> Optional[FileCoverage]:
        """Write one file's coverage measurement onto its FILE node (plain
        overwrite — a fresh measurement replaces the prior one outright,
        no anti-hallucination null-only guard; coverage is a point-in-time
        fact, not a PRD-extracted entity). Also derives and writes each of
        the file's FUNC/METHOD nodes' own `coverage_pct` by intersecting
        `uncovered_lines` against their known `line_start`/`line_end`
        ranges — no second measurement pass, just arithmetic over data
        AST extraction already produced. Returns the resulting FileCoverage
        (including the derived per-function breakdown), or None if no FILE
        node matches `file_path` in this project (nothing to attach the
        measurement to — the file must be indexed first)."""

    def get_coverage(self, file_path: str, subtree: str = "") -> Optional[FileCoverage]:
        """One file's latest coverage plus per-function breakdown, or None
        if the file was never measured (distinct from measured-at-0%)."""

    def coverage_report(
        self,
        subtree: str = "",
        below_pct: Optional[float] = None,
        file_glob: str = "",
        include_unmeasured: bool = True,
    ) -> list[FileCoverageSummary]:
        """Every FILE node's latest coverage, worst-first (ascending
        `coverage_pct`, unmeasured files sort first when included — the
        worst case, not a good one). `below_pct` filters to measured files
        under that threshold; unmeasured files are still included unless
        `include_unmeasured=False`, since "never tested" is itself an
        answer to "what needs better coverage." `file_glob` fnmatches
        `source_file`, matching `search_symbols`' existing convention."""

    def record_coverage_snapshot(self, snapshot: CoverageSnapshot) -> None:
        """Append one aggregate coverage measurement (its own
        `:CoverageSnapshot` node, not a :Node — a different entity kind,
        same precedent as `:AtomicTask`). Always a CREATE, never a MERGE:
        history is the point, each call adds a new snapshot rather than
        replacing the last one."""

    def coverage_trend(
        self, subtree: str = "", limit: int = 20
    ) -> list[CoverageSnapshot]:
        """Most recent `CoverageSnapshot`s for this project (optionally
        one `subtree`), most recent first. Empty list if none recorded
        yet."""
