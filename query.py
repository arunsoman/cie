"""Query engine: thin orchestration over a Repository.

The engine adds input validation and result shaping on top of the raw
repository contract. It has no knowledge of Neo4j, so it can be unit-tested
with an in-memory fake repository.
"""

from __future__ import annotations

from typing import Optional, Sequence

from cie.models import (
    ContextHit,
    CoverageSnapshot,
    EdgeRecord,
    FileCoverage,
    FileCoverageSummary,
    GraphStats,
    HybridMatch,
    MethodSignature,
    Node,
    NodeKind,
    NodeRecord,
    PathResult,
    SemanticMatch,
    SymbolMatch,
    TraversalMode,
    TraversalResult,
)
from cie.repository import Repository


class QueryEngine:
    """High-level query API used by the CLI and any future front-end."""

    def __init__(self, repo: Repository):
        self._repo = repo

    def search(
        self,
        question: str,
        mode: TraversalMode = TraversalMode.BFS,
        depth: int = 3,
    ) -> TraversalResult:
        terms = [t for t in question.split() if len(t) > 2]
        scored = self._repo.find_nodes_by_terms(terms)
        start_labels = [n.label for _, n in scored[:3]]
        if not start_labels:
            return TraversalResult(
                mode=mode, depth=depth, start_labels=(), nodes=(), edges=()
            )
        return self._repo.traverse(start_labels, mode, depth)

    def get_node(self, label_or_id: str) -> Optional[NodeRecord]:
        if not label_or_id:
            return None
        return self._repo.get_node(label_or_id)

    def get_neighbors(
        self, label_or_id: str, relation_filter: str = ""
    ) -> Optional[list[EdgeRecord]]:
        if not label_or_id:
            return None
        return self._repo.get_neighbors(label_or_id, relation_filter)

    def get_community(self, community_id: int) -> list[Node]:
        if community_id < 0:
            return []
        return self._repo.get_community(community_id)

    def list_communities(self) -> list[tuple[int, int]]:
        return self._repo.list_communities()

    def god_nodes(self, top_n: int = 10) -> list[NodeRecord]:
        if top_n <= 0:
            top_n = 10
        return self._repo.god_nodes(top_n)

    def stats(self) -> GraphStats:
        return self._repo.stats()

    def shortest_path(
        self, source: str, target: str, max_hops: int = 8
    ) -> Optional[PathResult]:
        if not source or not target:
            return None
        return self._repo.shortest_path(source, target, max_hops)

    # -- code-structure queries -------------------------------------------

    def get_signature(self, name: str) -> Optional[MethodSignature]:
        if not name:
            return None
        return self._repo.get_signature(name)

    def get_methods_of_class(self, class_name: str) -> list[MethodSignature]:
        if not class_name:
            return []
        return self._repo.get_methods_of_class(class_name)

    def discover_features(self) -> list[str]:
        return self._repo.discover_features()

    def list_files(self) -> list[Node]:
        return self._repo.list_files()

    def project_graph(self) -> tuple[list[Node], list[EdgeRecord]]:
        return self._repo.project_graph()

    # -- forge tool-layer queries (Wave B) ----------------------------------
    #
    # Thin wrappers: input guards + bounds clamping only. These NEVER raise
    # for not-found — they return empty lists and the surface layer attaches
    # the envelope hints (see cie.envelope).

    @staticmethod
    def _clamp_limit(limit: int) -> int:
        """Clamp a caller-supplied list cap into the allowed 1..50 window."""
        try:
            return max(1, min(int(limit), 50))
        except (TypeError, ValueError):
            return 50

    @staticmethod
    def _clamp_depth(depth: int) -> int:
        """Clamp a traversal depth into the allowed 1..6 window."""
        try:
            return max(1, min(int(depth), 6))
        except (TypeError, ValueError):
            return 3

    def search_symbols(
        self, name: str, kind: str = "", file_glob: str = "", limit: int = 20
    ) -> list[SymbolMatch]:
        """Find where a symbol is defined; empty list when nothing matches."""
        if not name or not name.strip():
            return []
        return self._repo.search_symbols(
            name.strip(), kind=kind, file_glob=file_glob,
            limit=self._clamp_limit(limit),
        )

    def get_callers(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Blast radius: edges from callers into `symbol`; [] when unknown."""
        if not symbol or not symbol.strip():
            return []
        return self._repo.get_callers(
            symbol.strip(), limit=self._clamp_limit(limit)
        )

    def get_callees(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Reverse localization: outgoing `calls` edges from `symbol`."""
        if not symbol or not symbol.strip():
            return []
        return self._repo.get_callees(
            symbol.strip(), limit=self._clamp_limit(limit)
        )

    def get_file_skeleton(self, path: str) -> list[Node]:
        """Signature/docstring/lines for every symbol in a file; [] if none."""
        if not path or not path.strip():
            return []
        return self._repo.get_file_skeleton(path.strip())

    def failing_context(
        self, test_identifier: str, depth: int = 3, limit: int = 30
    ) -> list[ContextHit]:
        """Symbols reachable from a failing test, nearest first; [] if unknown."""
        if not test_identifier or not test_identifier.strip():
            return []
        return self._repo.failing_context(
            test_identifier.strip(),
            depth=self._clamp_depth(depth),
            limit=self._clamp_limit(limit),
        )

    def affected_by(
        self, file_path: str, max_depth: int = 3, direction: str = "incoming",
        max_results: int = 30,
    ) -> list[ContextHit]:
        """Blast radius (docs/forge-rebuild-plan.md's Phase 2 / WS4, closes
        review finding P1.8): what depends on `file_path` (direction=
        "incoming", the default — "what breaks if I change this file",
        the repair agent's actual question) or what `file_path` depends on
        (direction="outgoing"). `max_results` is clamped into the same
        1..50 window every other capped list query uses in this class —
        the concrete guard against an expensive deep traversal on a large
        graph (closes P3.16's timeout/result-size-cap ask alongside
        `_clamp_depth`'s existing 1..6 window)."""
        if not file_path or not file_path.strip():
            return []
        if direction not in ("incoming", "outgoing"):
            direction = "incoming"
        return self._repo.affected_by(
            file_path.strip(), max_depth=self._clamp_depth(max_depth),
            direction=direction, max_results=self._clamp_limit(max_results),
        )

    def health_details(self) -> dict:
        """Backend connectivity + counts; propagates driver errors."""
        return self._repo.health_details()

    def semantic_search(self, query: str, top_k: int = 10) -> list[SemanticMatch]:
        """Rank nodes by embedding similarity to `query`; [] when the query
        is blank, embedding fails, or nothing has an embedding yet."""
        if not query or not query.strip():
            return []
        return self._repo.semantic_search(
            query.strip(), top_k=self._clamp_limit(top_k)
        )

    # -- RQ-01 hybrid retrieval -----------------------------------------------

    def hybrid_search(self, query: str, top_k: int = 10) -> list[HybridMatch]:
        """Combined lexical + dense + graph-degree ranked search; [] when
        `query` is blank."""
        if not query or not query.strip():
            return []
        return self._repo.hybrid_search(query.strip(), top_k=self._clamp_limit(top_k))

    # -- AI-02 entity-centric grounding ----------------------------------------

    def entity_context(self, symbol: str) -> dict:
        """One structured neighborhood block for `symbol` — a COMPOSITION
        of existing methods (`get_node`, `get_callers`, `get_callees`,
        `test_map`, and `class_hierarchy` when applicable), deliberately
        not a new Cypher traversal (per the task's own instruction: "it's
        a composition of what already exists"). This is why the method
        lives here, on `QueryEngine`, and NOT on `Repository`/
        `Neo4jRepository`/`InMemoryRepository` the way every other item in
        this slice does — there is no new persistence-layer query to add.

        `class_hierarchy` is populated when `symbol` resolves to a CLASS
        node (hierarchy of the class itself) OR a METHOD node (hierarchy
        of its ENCLOSING class, resolved via `get_signature`'s
        `enclosing_class` — `Node` itself carries no enclosing-class
        field, so this is the one existing method that already answers
        "what class is this method on"); `{}` for a FUNC/FILE/SYMBOL node,
        where "class hierarchy" doesn't apply.

        `{}` (not None) when `symbol` doesn't resolve to any node at all —
        matching this class's other not-found convention.
        """
        if not symbol or not symbol.strip():
            return {}
        symbol = symbol.strip()
        node_record = self.get_node(symbol)
        if node_record is None:
            return {}
        node = node_record.node

        hierarchy: dict = {}
        if node.kind == NodeKind.CLASS.value:
            hierarchy = self.class_hierarchy(node.label)
        elif node.kind == NodeKind.METHOD.value:
            signature = self.get_signature(symbol)
            if signature is not None and signature.enclosing_class:
                hierarchy = self.class_hierarchy(signature.enclosing_class)

        return {
            "node": node,
            "degree": node_record.degree,
            "callers": self.get_callers(symbol),
            "callees": self.get_callees(symbol),
            "tests": self.test_map(symbol),
            "class_hierarchy": hierarchy,
        }

    # -- DM-08 inheritance/interface -----------------------------------------

    def class_hierarchy(self, class_name: str) -> dict:
        """Ancestors/interfaces/descendants/implementers of a class or
        interface; `{}` when `class_name` is blank or unresolved."""
        if not class_name or not class_name.strip():
            return {}
        return self._repo.class_hierarchy(class_name.strip())

    # -- DM-14 test-to-implementation links ----------------------------------

    def test_map(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Tests covering `symbol`; [] when `symbol` is blank or unknown."""
        if not symbol or not symbol.strip():
            return []
        return self._repo.test_map(symbol.strip(), limit=self._clamp_limit(limit))

    # -- QA coverage (be-v2/docs/design/qa-persona-cie-knowledge-graph.md) --

    def record_coverage(
        self,
        file_path: str,
        subtree: str,
        coverage_pct: float,
        covered_lines: int,
        uncovered_lines,
        measured_at: str,
    ) -> Optional[FileCoverage]:
        if not file_path or not file_path.strip():
            return None
        return self._repo.record_coverage(
            file_path.strip(), subtree, coverage_pct, covered_lines,
            uncovered_lines, measured_at,
        )

    def get_coverage(self, file_path: str, subtree: str = "") -> Optional[FileCoverage]:
        if not file_path or not file_path.strip():
            return None
        return self._repo.get_coverage(file_path.strip(), subtree)

    def coverage_report(
        self,
        subtree: str = "",
        below_pct: Optional[float] = None,
        file_glob: str = "",
        include_unmeasured: bool = True,
    ) -> list[FileCoverageSummary]:
        return self._repo.coverage_report(
            subtree, below_pct, file_glob, include_unmeasured,
        )

    def record_coverage_snapshot(self, snapshot: CoverageSnapshot) -> None:
        self._repo.record_coverage_snapshot(snapshot)

    def coverage_trend(self, subtree: str = "", limit: int = 20) -> list[CoverageSnapshot]:
        return self._repo.coverage_trend(subtree, self._clamp_limit(limit))
