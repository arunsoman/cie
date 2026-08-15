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
    HybridMatch,
    MethodSignature,
    MetricSnapshot,
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

    # -- RQ-01 hybrid retrieval -----------------------------------------------

    def hybrid_search(
        self, query: str, top_k: int = 10, project: str = "",
    ) -> list[HybridMatch]:
        """Combined lexical + dense + graph-degree ranked search; [] when
        `query` is blank or every leg finds nothing. See
        `Neo4jRepository.hybrid_search` for the weighting/normalization
        details."""

    # -- DM-08 inheritance/interface -----------------------------------------

    def class_hierarchy(self, class_name: str) -> dict:
        """Ancestors/interfaces/descendants/implementers of a class or
        interface, resolved from `extends`/`implements` edges. `{}` when
        `class_name` doesn't resolve to any node. See
        `Neo4jRepository.class_hierarchy` for the transitive-vs-direct
        scoping of each key."""

    # -- DM-14 test-to-implementation links ----------------------------------

    def test_map(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Tests covering `symbol`, resolved from `TESTS` edges (see
        `cie.testlink.resolve_test_edges`). Empty list (not None) when
        `symbol` is unknown or has no linked test."""

    # -- Section 13 (Code Intelligence) shared plumbing -----------------------

    def code_symbol_nodes(self, project: str = "") -> list[Node]:
        """Every FUNC/METHOD node with a real `source_file` — the
        candidate set `cie.clone_detect`/`cie.perf_analyze` re-parse from
        disk. See `Neo4jRepository.code_symbol_nodes` for the exact
        exclusions (CLASS/FILE/SYMBOL nodes, section 13's own analysis
        kinds)."""

    def analysis_nodes(self, kind: str, project: str = "") -> list[Node]:
        """Every existing node of one section-13 analysis kind
        (CloneCluster/AntiPattern/DriftFinding) — read side of
        `replace_analysis_nodes`."""

    def replace_analysis_nodes(
        self, kind: str, nodes: Sequence[dict], edges: Sequence[dict],
        project: str = "",
    ) -> int:
        """Idempotent replace of ONE analysis kind's nodes/edges — deletes
        existing `:Node {kind: kind}` (scoped to `project`) before writing
        fresh ones, leaving every other node kind (including OTHER
        analysis kinds) untouched. See `Neo4jRepository.
        replace_analysis_nodes` for the full write-path contract."""

    def embedding_clone_pairs(
        self, threshold: float = 0.92, k: int = 5, project: str = "",
    ) -> list[tuple[str, str, float]]:
        """CI-03: `(id_a, id_b, score)` triples for FUNC/METHOD node pairs
        whose stored embeddings are cosine-similar at or above
        `threshold`. See `Neo4jRepository.embedding_clone_pairs`."""

    def update_node_properties(self, updates: dict[str, dict], project: str = "") -> int:
        """CI-06/08: patch EXISTING nodes (by id) with extra properties
        (`complexity_class`, `hot_path`), without touching any other
        field. See `Neo4jRepository.update_node_properties`."""

    def record_metric_snapshot(self, snapshot: MetricSnapshot) -> None:
        """CI-19: append one aggregate-metric measurement (append-only,
        never replaced). See `Neo4jRepository.record_metric_snapshot`."""

    def metric_trend(
        self, metric_type: str = "", limit: int = 20, project: str = "",
    ) -> list[MetricSnapshot]:
        """CI-21: historical `MetricSnapshot`s, most recent first. See
        `Neo4jRepository.metric_trend`."""

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

    # -- Section 0 (Population & Real-Time Sync) write primitives -----------

    def merge_delta(
        self, nodes: Sequence[dict], edges: Sequence[dict], project: str = "",
    ) -> int:
        """PS-02/PS-16: idempotent MERGE-on-id write, never a delete.

        Unlike `load_extraction` (whole-project replace) and
        `replace_analysis_nodes` (whole-kind replace), this only ever
        MERGEs: an existing node with the same `id` (scoped to `project`)
        gets its properties overwritten (`ON MATCH SET n = row`), a new
        one gets created. This is the promotion primitive (PS-02):
        copying a speculative graph's nodes/edges into the canonical
        namespace must not wipe out canonical nodes that came from a
        DIFFERENT file's promotion earlier in the same session. Also
        backs PS-16 (`load_commit`): re-running against the same commit
        re-MERGEs the same rows, so the result is identical (true
        idempotency, not just "replace produces the same thing every
        time" which `load_extraction` already gave structurally-extracted
        data). Edges MERGE on `(source, target, relation)` for the same
        reason. Returns the number of node rows written.
        """

    def delete_nodes(self, ids: Sequence[str], project: str = "") -> int:
        """Delete nodes by id (DETACH DELETE — also removes their edges).
        Generic primitive; PS-01 uses it for speculative-graph TTL
        eviction. Returns the number of ids that actually matched an
        existing node (scoped to `project` when given)."""

    def delete_nodes_before(self, ids: Sequence[str], cutoff_iso: str, project: str = "") -> int:
        """SF4: single-transaction, timestamp-filtered variant of
        `delete_nodes` — deletes only the nodes in `ids` whose own
        `extracted_at` is STILL `< cutoff_iso` at delete time (re-checked
        server-side, in the SAME transaction as the delete, not read
        separately beforehand). `evict_stale_speculative` (PS-01) uses
        this instead of `delete_nodes` for exactly the race
        `delete_nodes` alone can't close: computing a `stale_ids` list
        from a point-in-time snapshot, then deleting by id in a SEPARATE
        call, means a concurrent `FILE_SAVE` sync event that refreshes
        one of those same node ids in between gets its refresh silently
        discarded — the id was already captured as stale before the
        refresh landed. Re-checking `extracted_at` inside the same delete
        transaction closes that window. Returns the number of nodes
        actually deleted (a node whose `extracted_at` was refreshed past
        `cutoff_iso` before this ran is correctly NOT counted)."""

    # -- CI-15/16/17 runtime telemetry (section 13.4) ------------------------

    def accumulate_actual_calls(
        self, pairs: Sequence[dict], project: str = "",
    ) -> int:
        """CI-15: atomically fold a batch of observed (caller, callee)
        span pairs into `ACTUAL_CALL` edges. Each `pairs` row is
        `{"source": node_id, "target": node_id, "latency_ms": float,
        "is_error": bool}`. Unlike `merge_delta` (whose `SET r = row`
        would overwrite a running counter with a single new
        measurement), this is a real read-increment-write per pair:
        `call_count`/`total_latency_ms`/`error_count` accumulate across
        every ingestion this project has ever received, and
        `avg_latency_ms`/`error_rate` are recomputed from the running
        totals on every write. One `ACTUAL_CALL` edge per (caller,
        callee) pair, not one per span — repeated calls between the same
        two symbols strengthen the same edge rather than creating
        duplicates. Returns the number of pair rows processed (not
        necessarily distinct edges, same approximate-count convention as
        `merge_delta`)."""

    def actual_callers(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """CI-16: functions that ACTUALLY called `symbol` at runtime
        (per ingested telemetry), with `call_count`/`avg_latency_ms`/
        `error_rate` on each `EdgeRecord`'s `edge.properties` — distinct
        from `get_callers`, which returns statically POSSIBLE callers
        from the AST call graph. Empty list (not None) when `symbol` is
        unknown or has never been observed calling anything at runtime."""

    # -- TE-09 durable telemetry queue ---------------------------------------

    def publish_telemetry_event(
        self, channel: str, payload: dict, project: str = "",
    ) -> int:
        """TE-09: append one event to a durable, per-`(project, channel)`
        telemetry log (its own `:TelemetryEvent` node, CREATE-only — same
        append-only precedent as `record_metric_snapshot`/
        `record_coverage_snapshot`) and return its assigned sequence
        number (monotonically increasing per `(project, channel)`, via
        a `:TelemetryCounter` node incremented in the same write). This
        is the persistence half of `cie.apm.TelemetryBus` — see that
        class's docstring for why this is deliberately NOT a new message
        broker (NATS/Kafka): the existing Neo4j database serves as the
        append-only log instead."""

    def read_telemetry_events(
        self, channel: str, after_seq: int = 0, limit: int = 100, project: str = "",
    ) -> list[dict]:
        """TE-09: events on `channel` with `seq > after_seq`, oldest
        first, capped at `limit` — the replay/resume read side of
        `publish_telemetry_event`. Each dict: `{seq, channel, payload,
        published_at}`. Lets a subscriber that missed live delivery
        (crashed, or just started after the event was published) catch
        up from the durable log instead of losing it."""

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
