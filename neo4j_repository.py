"""Neo4j-backed implementation of the Repository protocol.

All graph operations are expressed as Cypher queries against a Neo4j
database. The schema mirrors the original cie extraction output:

    (n:Node {
        id, label, source_file, source_location, file_type, community
    })
    (a)-[:RELATES {relation, confidence, _source, _target}]->(b)

Edges are stored as directed relationships but queried in both directions
to match cie's undirected traversal semantics. The `_source`/`_target`
attributes preserve the original extraction direction.
"""

from __future__ import annotations

import fnmatch
import logging
import os
from typing import Optional, Sequence

from neo4j import Driver, GraphDatabase, Query

from core.llm.embed_text import embed_text
from cie.embed import compute_embeddings
from cie.extract import Extraction
from cie.timeouts import Neo4jOperationTimeout, run_with_timeout

from cie.models import (
    Confidence,
    ContextHit,
    CoverageSnapshot,
    Edge,
    EdgeRecord,
    FileCoverage,
    FileCoverageSummary,
    FunctionCoverage,
    GraphStats,
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

logger = logging.getLogger("cie.neo4j_repository")


# Node labels that are synthetic hubs (file-level or AST stubs) and should be
# excluded from "god node" ranking. Mirrors cie/analyze._is_file_node.
_FILE_EXTENSIONS = (
    "py", "ts", "js", "go", "rs", "java", "rb", "cpp", "c", "h",
)


# Idempotent schema setup: every equality/lookup predicate the Repository's
# Cypher relies on (project scoping via `_pw`/`_pw_where`, id/source_file
# lookups in reindex_file, kind filters in search_symbols/get_file_skeleton)
# gets a matching index. The fulltext index backs substring-style label
# search; query methods still use `CONTAINS` today (see find_nodes_by_terms
# and friends), so it is not yet consulted by the planner — it exists so a
# future switch to `db.index.fulltext.queryNodes` doesn't need a schema
# migration. `IF NOT EXISTS` makes every statement safe to re-run.
_INDEX_STATEMENTS = (
    "CREATE INDEX node_id_idx IF NOT EXISTS FOR (n:Node) ON (n.id)",
    "CREATE INDEX node_project_idx IF NOT EXISTS FOR (n:Node) ON (n.project)",
    "CREATE INDEX node_source_file_idx IF NOT EXISTS FOR (n:Node) ON (n.source_file)",
    "CREATE INDEX node_kind_idx IF NOT EXISTS FOR (n:Node) ON (n.kind)",
    "CREATE INDEX node_label_idx IF NOT EXISTS FOR (n:Node) ON (n.label)",
    "CREATE INDEX node_community_idx IF NOT EXISTS FOR (n:Node) ON (n.community)",
    "CREATE INDEX relates_relation_idx IF NOT EXISTS FOR ()-[r:RELATES]-() ON (r.relation)",
    "CREATE FULLTEXT INDEX node_label_fulltext IF NOT EXISTS FOR (n:Node) ON EACH [n.label]",
    "CREATE INDEX node_coverage_pct_idx IF NOT EXISTS FOR (n:Node) ON (n.coverage_pct)",
    "CREATE INDEX coverage_snapshot_project_idx IF NOT EXISTS FOR (s:CoverageSnapshot) ON (s.project)",
)


# Embedding dimension for the default embeddings model
# (`core.llm.embed_text`'s `nvidia/nv-embedqa-e5-v5`, via NVIDIA NIM's
# OpenAI-compatible endpoint). Switched 2026-08-03 from `baai/bge-m3`,
# whose NVIDIA-hosted deployment was confirmed permanently returning a
# server-side 500 on every request (`nvcf-status: errored`), not a
# transient outage — `nv-embedqa-e5-v5` is confirmed live on the same
# account/endpoint and is also 1024-dim, so this constant didn't need to
# change. If the deployed model ever changes to something with a
# different dimension, this constant AND the index below both need to
# change together, or vector queries silently return nothing.
EMBEDDING_DIMENSION = 1024

# Applied separately from `_INDEX_STATEMENTS` (still folded into
# `ensure_indices()` below) so the dimension constant has one obvious home.
_VECTOR_INDEX_STATEMENTS = (
    "CREATE VECTOR INDEX node_embedding_idx IF NOT EXISTS FOR (n:Node) ON (n.embedding) "
    "OPTIONS {indexConfig: {`vector.dimensions`: %d, "
    "`vector.similarity_function`: 'cosine'}}" % EMBEDDING_DIMENSION,
)


def _row_to_node(row: dict) -> Node:
    return Node(
        id=row["id"],
        label=row.get("label", row["id"]),
        source_file=row.get("source_file", ""),
        source_location=row.get("source_location", ""),
        file_type=row.get("file_type", ""),
        community=row.get("community"),
        kind=row.get("kind", NodeKind.SYMBOL.value),
        signature=row.get("signature", ""),
        line_start=row.get("line_start") or 0,
        line_end=row.get("line_end") or 0,
        docstring=row.get("docstring", ""),
        project=row.get("project", ""),
        embedding=tuple(row.get("embedding") or ()),
    )


def _row_to_edge_record(row: dict) -> EdgeRecord:
    rel = row["rel"]
    conf_raw = rel.get("confidence", "EXTRACTED")
    try:
        confidence = Confidence(conf_raw)
    except ValueError:
        confidence = Confidence.EXTRACTED
    edge = Edge(
        source=rel.get("_source", row["source_id"]),
        target=rel.get("_target", row["target_id"]),
        relation=rel.get("relation", ""),
        confidence=confidence,
    )
    return EdgeRecord(
        edge=edge,
        source_label=row.get("source_label", edge.source),
        target_label=row.get("target_label", edge.target),
    )


def _is_file_hub(label: str) -> bool:
    """Match cie's exclusion of file-level hub and AST stub nodes."""
    if not label:
        return False
    if label.split(".")[-1] in _FILE_EXTENSIONS:
        return True
    if label.startswith(".") and label.endswith("()"):
        return True
    return False


def _parse_params(signature: str) -> tuple[str, ...]:
    """Extract parameter names from a signature string like 'foo(a, b: int) -> str'."""
    if not signature:
        return ()
    start = signature.find("(")
    end = signature.rfind(")")
    if start == -1 or end == -1 or end <= start:
        return ()
    inner = signature[start + 1:end].strip()
    if not inner:
        return ()
    params: list[str] = []
    for raw in inner.split(","):
        raw = raw.strip()
        if not raw:
            continue
        # strip type annotation: 'b: int' -> 'b'
        name = raw.split(":")[0].strip()
        # strip default: 'b = 1' -> 'b'
        name = name.split("=")[0].strip()
        if name:
            params.append(name)
    return tuple(params)


def _parse_return_type(signature: str) -> str:
    """Extract the return type after '->' in a signature, or ''."""
    if not signature:
        return ""
    idx = signature.rfind("->")
    if idx == -1:
        return ""
    return signature[idx + 2:].strip()


# LLM-friendly description of every querying feature the engine exposes.
# Used by the `discover` command so an agent can learn the system's
# capabilities without reading source code.
_FEATURE_DESCRIPTIONS = [
    "search \"<question or keywords>\" [--mode bfs|dfs] [--depth 1-6]: "
    "Score nodes against keywords, then traverse the graph (BFS for broad "
    "context, DFS to trace a specific path) and return the reachable nodes "
    "and edges.",
    "node <label or id>: Get full details for a single node - label, source "
    "file, location, kind, community, and degree.",
    "neighbors <label> [--relation <type>]: List all direct neighbors of a "
    "node with the edge relation and confidence (EXTRACTED/INFERRED/AMBIGUOUS).",
    "community <id>: Return every node in a community by its numeric id.",
    "communities: List all communities ordered by size (largest first).",
    "god [--top-n N]: Return the most-connected real entities - the core "
    "abstractions of the codebase. File-level hubs are excluded.",
    "stats: Whole-graph summary - node count, edge count, community count, "
    "and the EXTRACTED/INFERRED/AMBIGUOUS confidence breakdown.",
    "path <source> <target> [--max-hops N]: Find the shortest path between "
    "two concepts and render it as a labeled hop sequence.",
    "signature <name>: Return the signature (name, parameters, return type, "
    "enclosing class) of a function or method by name.",
    "methods <class name>: Return all methods declared in a given class, "
    "each with its signature.",
    "files: List all source files that have been loaded into the graph.",
    "discover: Return this list of features so an agent can learn what the "
    "system can do without reading its source.",
    "semantic-search \"<question>\" [--top-k N]: Rank nodes by cosine "
    "similarity between an embedded query and each node's stored embedding "
    "- finds conceptually related code even without shared keywords. "
    "Complements `search`'s lexical/substring matching. Empty results mean "
    "no nodes have embeddings yet (requires NVIDIA_API_KEY set at load time).",
]


class Neo4jRepository:
    """Repository implementation backed by a Neo4j database.

    The driver is created lazily from connection settings so the object can be
    instantiated in tests without a running database. Callers should use the
    classmethod :meth:`connect` for production use and :meth:`close` when done.

    When `project` is non-empty, every read query restricts its ``:Node``
    matches to that project namespace; nodes without a `project` property
    belong to the default ("") project only.
    """

    def __init__(
        self,
        driver: Driver,
        project: str = "",
        query_timeout_s: float = 30.0,
        schema_timeout_s: float = 15.0,
        write_timeout_s: float = 120.0,
    ):
        self._driver = driver
        self._project = project
        self._query_timeout_s = query_timeout_s
        self._schema_timeout_s = schema_timeout_s
        self._write_timeout_s = write_timeout_s

    @classmethod
    def connect(
        cls,
        uri: str,
        user: str,
        password: str,
        database: str = "neo4j",
        project: str = "",
        ensure_indices: bool = False,
        connect_timeout_s: float = 10.0,
        max_retry_time_s: float = 15.0,
        query_timeout_s: float = 30.0,
        schema_timeout_s: float = 15.0,
        write_timeout_s: float = 120.0,
    ) -> "Neo4jRepository":
        driver = GraphDatabase.driver(
            uri,
            auth=(user, password),
            connection_timeout=connect_timeout_s,
            max_transaction_retry_time=max_retry_time_s,
        )
        repo = cls(
            driver,
            project=project,
            query_timeout_s=query_timeout_s,
            schema_timeout_s=schema_timeout_s,
            write_timeout_s=write_timeout_s,
        )
        if ensure_indices:
            repo.ensure_indices()
        return repo

    def ensure_indices(self) -> None:
        """Create every index in `_INDEX_STATEMENTS` if it doesn't exist yet.

        Safe to call on every connect: `IF NOT EXISTS` makes each statement a
        no-op once the index is already there — *except* that "no-op" still
        requires acquiring Neo4j's exclusive schema lock, which queues
        behind any other transaction (even a read) holding the shared
        schema lock. Confirmed live 2026-08-04: a multi-hour leaked read
        transaction on this repo's shared Aura instance blocked this call
        indefinitely with no error. Bounded by `run_with_timeout`; a
        timeout here is treated as non-fatal (the indices this call would
        create/no-op almost certainly already exist from a prior
        successful run — that was true in the confirmed incident) so one
        stuck transaction elsewhere doesn't take down every read command
        that happens to connect.
        """
        def _create_all() -> None:
            with self._driver.session() as session:
                for stmt in _INDEX_STATEMENTS + _VECTOR_INDEX_STATEMENTS:
                    session.run(stmt).consume()

        try:
            run_with_timeout(
                _create_all,
                timeout=self._schema_timeout_s,
                description="ensure_indices (CREATE INDEX IF NOT EXISTS)",
            )
        except Neo4jOperationTimeout as exc:
            logger.warning(
                "ensure_indices timed out and was skipped (non-fatal — "
                "indices most likely already exist): %s", exc,
            )

    def close(self) -> None:
        self._driver.close()

    def __enter__(self) -> "Neo4jRepository":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    # -- helpers -----------------------------------------------------------

    def _run(self, query: str, params: Optional[dict] = None):
        """``query`` is usually a plain Cypher string, but `affected_by`/
        `failing_context` pass a `neo4j.Query` object instead (to carry a
        server-side timeout — see those methods' docstrings). A `Query`
        isn't subscriptable, so `query[:80]` for the timeout-error
        description crashed unconditionally the moment either method ran
        for real — confirmed live: no test exercised `Neo4jRepository.
        affected_by` itself (only the `QueryEngine`/`ToolService` layers
        above it, against a fake repo) so this had never been caught.

        Runs through ``session.execute_read`` rather than a bare
        ``session.run`` — confirmed live 2026-08-07: this is forge's tool-
        call hot path (every search/view/blame during agentic generation
        goes through here), and a bare auto-commit ``session.run`` gets
        NO automatic retry from the driver on a transient/routing failure
        (``ServiceUnavailable``/"Unable to retrieve routing information"),
        unlike the write paths below that already use ``execute_write``.
        A managed read transaction retries automatically, bounded by
        ``max_transaction_retry_time`` (``Neo4jConfig.max_retry_time_s``,
        driver-level, set in ``driver_kwargs()``) — the same bound writes
        already get — instead of raising the first time Aura's routing
        table is momentarily stale.

        EXCEPT for a ``Query`` object: confirmed live 2026-08-07 (second
        incident, same day) — the driver only accepts a ``Query`` (which
        carries a server-side timeout) via ``Session.run``; a managed
        transaction's ``Transaction.run`` raises
        ``TypeError: Query object is only supported for session.run``
        unconditionally. `affected_by`/`failing_context` need that
        server-side timeout more than they need routing retry, so they
        keep the bare ``session.run`` path; every plain-string query gets
        the retry-capable managed transaction.
        """
        merged = dict(params or {})
        merged.setdefault("project", self._project)
        query_text = query.text if isinstance(query, Query) else query

        def _exec() -> list:
            with self._driver.session() as session:
                if isinstance(query, Query):
                    return list(session.run(query, merged))
                return list(session.execute_read(lambda tx: list(tx.run(query, merged))))

        return run_with_timeout(
            _exec, timeout=self._query_timeout_s,
            description=f"query {query_text[:80]!r}",
        )

    def _pw(self, alias: str) -> str:
        """Project filter fragment: ' AND <alias>.project = $project' or ''."""
        if not self._project:
            return ""
        return f" AND {alias}.project = $project"

    def _pw_where(self, alias: str) -> str:
        """Project filter as a standalone WHERE clause, or ''."""
        if not self._project:
            return ""
        return f" WHERE {alias}.project = $project"

    # -- Repository methods -------------------------------------------------

    def find_nodes_by_terms(self, terms: Sequence[str]) -> list[tuple[float, Node]]:
        if not terms:
            return []
        lowered = [t.lower() for t in terms]
        query = """
        UNWIND $terms AS t
        MATCH (n:Node)""" + self._pw_where("n") + """
        WITH n, t,
             CASE WHEN toLower(n.label) CONTAINS t THEN 1.0 ELSE 0.0 END AS label_hit,
             CASE WHEN toLower(n.source_file) CONTAINS t THEN 0.5 ELSE 0.0 END AS file_hit
        WITH n, sum(label_hit) + sum(file_hit) AS score
        WHERE score > 0
        RETURN n
        ORDER BY score DESC
        """
        rows = self._run(query, {"terms": lowered})
        # Aggregate scores per node (UNWIND produces one row per term hit).
        scores: dict[str, float] = {}
        nodes: dict[str, Node] = {}
        for row in rows:
            node = _row_to_node(row["n"])
            scores[node.id] = scores.get(node.id, 0.0) + 1.0
            nodes[node.id] = node
        ranked = sorted(scores.items(), key=lambda kv: kv[1], reverse=True)
        return [(score, nodes[nid]) for nid, score in ranked]

    def get_node(self, label_or_id: str) -> Optional[NodeRecord]:
        needle = label_or_id.lower()
        query = """
        MATCH (n:Node)
        WHERE (toLower(n.label) CONTAINS $needle OR toLower(n.id) = $needle)""" + self._pw("n") + """
        OPTIONAL MATCH (n)-[r]-()
        WITH n, count(r) AS degree
        RETURN n, degree
        LIMIT 1
        """
        rows = self._run(query, {"needle": needle})
        if not rows:
            return None
        row = rows[0]
        return NodeRecord(node=_row_to_node(row["n"]), degree=row["degree"])

    def get_neighbors(
        self, label_or_id: str, relation_filter: str = ""
    ) -> Optional[list[EdgeRecord]]:
        needle = label_or_id.lower()
        match_clause = """
        MATCH (n:Node)
        WHERE (toLower(n.label) CONTAINS $needle OR toLower(n.id) = $needle)""" + self._pw("n") + """
        WITH n LIMIT 1
        MATCH (n)-[rel]-(m:Node)"""
        if self._project:
            match_clause += " WHERE m.project = $project\n"
        match_clause += "\n"
        if relation_filter:
            conj = "AND" if self._project else "WHERE"
            match_clause += f" {conj} toLower(rel.relation) CONTAINS $rel_filter\n"
            params = {"needle": needle, "rel_filter": relation_filter.lower()}
        else:
            params = {"needle": needle}
        query = (
            match_clause
            + """
            RETURN rel, n.id AS source_id, n.label AS source_label,
                   m.id AS target_id, m.label AS target_label
            """
        )
        rows = self._run(query, params)
        if not rows:
            # Distinguish "node not found" from "node has no neighbors".
            exists = self._run(
                "MATCH (n:Node) WHERE (toLower(n.label) CONTAINS $needle "
                "OR toLower(n.id) = $needle)" + self._pw("n")
                + " RETURN n LIMIT 1",
                {"needle": needle},
            )
            return None if not exists else []
        return [_row_to_edge_record(r) for r in rows]

    def get_community(self, community_id: int) -> list[Node]:
        query = """
        MATCH (n:Node {community: $cid})""" + self._pw_where("n") + """
        RETURN n ORDER BY n.label
        """
        rows = self._run(query, {"cid": community_id})
        return [_row_to_node(r["n"]) for r in rows]

    def list_communities(self) -> list[tuple[int, int]]:
        query = """
        MATCH (n:Node)
        WHERE n.community IS NOT NULL""" + self._pw("n") + """
        RETURN n.community AS cid, count(*) AS size
        ORDER BY size DESC, cid
        """
        rows = self._run(query)
        return [(r["cid"], r["size"]) for r in rows]

    def god_nodes(self, top_n: int = 10) -> list[NodeRecord]:
        """Previously filtered file-hub/AST-stub labels partly in Cypher
        (extension suffixes) and partly in Python (`_is_file_hub`, applied
        AFTER the Cypher `LIMIT $top_n`) — meaning a result that hit one of
        the two Python-only cases (a bare extension with no leading dot,
        e.g. a malformed label literally "py") could silently return fewer
        than `top_n` rows even when more real god nodes existed just past
        the limit window. `$extensions` below reproduces `_is_file_hub`
        exactly (dotted-suffix labels AND bare-extension labels), so the
        exclusion is now applied before the LIMIT, not after — see
        `_is_file_hub`'s own docstring, which this must stay in sync with.
        """
        query = """
        MATCH (n:Node)
        WHERE NOT n.label IN $extensions
          AND NOT any(ext IN $extensions WHERE n.label ENDS WITH '.' + ext)
          AND NOT n.label STARTS WITH '.'""" + self._pw("n") + """
        OPTIONAL MATCH (n)-[r]-()
        WITH n, count(r) AS degree
        WHERE degree > 0
        RETURN n, degree
        ORDER BY degree DESC
        LIMIT $top_n
        """
        rows = self._run(query, {"top_n": top_n, "extensions": list(_FILE_EXTENSIONS)})
        return [NodeRecord(node=_row_to_node(r["n"]), degree=r["degree"]) for r in rows]

    def stats(self) -> GraphStats:
        """One round trip instead of four. Chained via `WITH` (not `CALL {}`
        subqueries) deliberately: a `CALL {}` subquery doing a GROUPED
        aggregate (confidence -> count) collapses to zero rows when there
        are zero matching edges, which would have zero'd out the whole
        combined result including the unrelated node/community counts —
        `collect(r.confidence)` instead returns an (possibly empty) list in
        the same single scalar row every `count()` aggregate already
        guarantees, and the per-confidence-value counting happens in Python
        over that (small) list.
        """
        if self._project:
            edge_clause = (
                "OPTIONAL MATCH (a:Node)-[r]->(b:Node) "
                "WHERE a.project = $project AND b.project = $project"
            )
        else:
            edge_clause = "OPTIONAL MATCH ()-[r]->()"
        query = (
            "MATCH (n:Node)" + self._pw_where("n") + "\n"
            "WITH count(n) AS nodes\n"
            + edge_clause + "\n"
            "WITH nodes, count(r) AS edges, collect(r.confidence) AS confidences\n"
            "OPTIONAL MATCH (c:Node) WHERE c.community IS NOT NULL" + self._pw("c") + "\n"
            "RETURN nodes, edges, confidences, count(DISTINCT c.community) AS communities"
        )
        rows = self._run(query)
        if not rows:
            return GraphStats(nodes=0, edges=0, communities=0, confidence_counts={})
        row = rows[0]
        confidence_counts: dict[str, int] = {}
        for conf in row["confidences"] or []:
            key = conf or "EXTRACTED"
            confidence_counts[key] = confidence_counts.get(key, 0) + 1
        return GraphStats(
            nodes=row["nodes"] or 0,
            edges=row["edges"] or 0,
            communities=row["communities"] or 0,
            confidence_counts=confidence_counts,
        )

    def shortest_path(
        self, source: str, target: str, max_hops: int = 8
    ) -> Optional[PathResult]:
        src = self.find_nodes_by_terms(source.split())
        tgt = self.find_nodes_by_terms(target.split())
        if not src or not tgt:
            return None
        src_id = src[0][1].id
        tgt_id = tgt[0][1].id
        if self._project:
            query = """
            MATCH (s:Node {id: $src, project: $project}),
                  (t:Node {id: $tgt, project: $project}),
                  p = shortestPath((s)-[*..8]-(t))
            RETURN p
            """
        else:
            query = """
            MATCH (s:Node {id: $src}), (t:Node {id: $tgt}),
                  p = shortestPath((s)-[*..8]-(t))
            RETURN p
            """
        rows = self._run(query, {"src": src_id, "tgt": tgt_id})
        if not rows:
            return None
        path = rows[0]["p"]
        nodes: list[Node] = []
        edges: list[EdgeRecord] = []
        rels = path.relationships
        node_seq = path.nodes
        for i, node in enumerate(node_seq):
            props = dict(node)
            nodes.append(_row_to_node(props))
        for i, rel in enumerate(rels):
            conf_raw = rel.get("confidence", "EXTRACTED")
            try:
                confidence = Confidence(conf_raw)
            except ValueError:
                confidence = Confidence.EXTRACTED
            edges.append(
                EdgeRecord(
                    edge=Edge(
                        source=rel.get("_source", dict(node_seq[i])["id"]),
                        target=rel.get("_target", dict(node_seq[i + 1])["id"]),
                        relation=rel.get("relation", ""),
                        confidence=confidence,
                    ),
                    source_label=dict(node_seq[i]).get("label", ""),
                    target_label=dict(node_seq[i + 1]).get("label", ""),
                )
            )
        hops = len(rels)
        if hops > max_hops:
            return None
        return PathResult(hops=hops, nodes=tuple(nodes), edges=tuple(edges))

    def traverse(
        self,
        start_labels: Sequence[str],
        mode: TraversalMode,
        depth: int,
    ) -> TraversalResult:
        depth = max(1, min(int(depth), 6))
        start_ids: list[str] = []
        start_label_list: list[str] = []
        for label in start_labels:
            scored = self.find_nodes_by_terms(label.split())
            if scored:
                start_ids.append(scored[0][1].id)
                start_label_list.append(scored[0][1].label)
        if not start_ids:
            return TraversalResult(
                mode=mode,
                depth=depth,
                start_labels=tuple(start_labels),
                nodes=(),
                edges=(),
            )
        # Use APOC-free variable-length path expansion. Both directions to
        # match cie's undirected traversal. Collect distinct nodes/edges.
        query = """
        MATCH (s:Node)
        WHERE s.id IN $start_ids""" + self._pw("s") + """
        MATCH p = (s)-[*..%d]-(m:Node)""" % depth
        if self._project:
            query += "\nWHERE m.project = $project\n"
        query += """
        WITH p
        LIMIT 10000
        UNWIND nodes(p) AS n
        UNWIND relationships(p) AS rel
        WITH n, rel, startNode(rel) AS a, endNode(rel) AS b
        RETURN DISTINCT n, rel, a.id AS source_id, a.label AS source_label,
                        b.id AS target_id, b.label AS target_label
        """
        rows = self._run(query, {"start_ids": start_ids})
        seen_nodes: dict[str, Node] = {}
        seen_edges: dict[tuple[str, str, str], EdgeRecord] = {}
        for r in rows:
            node = _row_to_node(r["n"])
            seen_nodes[node.id] = node
            er = _row_to_edge_record(r)
            key = (er.edge.source, er.edge.target, er.edge.relation)
            seen_edges.setdefault(key, er)
        return TraversalResult(
            mode=mode,
            depth=depth,
            start_labels=tuple(start_label_list),
            nodes=tuple(seen_nodes.values()),
            edges=tuple(seen_edges.values()),
        )

    # -- loader -----------------------------------------------------------

    @staticmethod
    def _stamped_nodes(nodes: Sequence[dict], project: str) -> list[dict]:
        """Copy node dicts, stamping the project namespace when non-empty."""
        rows = [dict(n) for n in nodes]
        if project:
            for row in rows:
                row["project"] = project
        return rows

    @staticmethod
    def _maybe_compute_embeddings(rows: list[dict]) -> None:
        """Compute embeddings for freshly-stamped node rows, guarded on
        NVIDIA_API_KEY so `load`/`reindex` don't hard-depend on the
        embeddings API being reachable (e.g. in a test environment with no
        real credentials). When skipped, every row still gets an explicit
        `embedding: []` so downstream reads see a consistent shape.
        """
        if not rows:
            return
        if not os.environ.get("NVIDIA_API_KEY"):
            logger.info(
                "NVIDIA_API_KEY not set; skipping embedding computation for "
                "%d node(s)",
                len(rows),
            )
            for row in rows:
                row.setdefault("embedding", [])
            return
        compute_embeddings(rows)

    def load_extraction(
        self,
        nodes: Sequence[dict],
        edges: Sequence[dict],
        project: str = "",
    ) -> int:
        """Idempotent full load; returns the number of nodes written.

        With `project` empty the whole graph is replaced (legacy behaviour).
        With a non-empty `project`, only that project's nodes are deleted
        (other projects in the same database are untouched) and every node is
        stamped with the project namespace. Edge rows keep their `line`
        property when present.

        The delete + recreate now runs as ONE `session.execute_write`
        transaction (matching `reindex_file`'s own pattern below) instead
        of three separate auto-committed `session.run` calls. Each bare
        `session.run` outside an explicit transaction auto-commits on its
        own in the neo4j driver's default mode — the delete and the
        recreate were never atomic, so a reader hitting the database
        between them saw a genuinely empty graph, and a failure after the
        delete but before the recreate finished left the project's graph
        empty with no rollback. A single transaction removes both: nothing
        is visible to another session until the whole write commits, and
        an error mid-write rolls back to the pre-delete state instead of
        leaving the graph half-loaded.
        """
        rows = self._stamped_nodes(nodes, project)
        self._maybe_compute_embeddings(rows)
        edge_rows = [dict(e) for e in edges]

        def _tx(tx) -> None:
            if project:
                # Label-scoped deliberately: core.graph.repository's
                # save_bev2_entity also stamps a `project` property (not
                # `project_id`) on the business-entity nodes it writes
                # (:Module/:Actor/etc, a different schema from this
                # module's own :Node), and Cypher MERGE's label-subset
                # matching means a PRD-derived :Entity:Module node can
                # end up carrying BOTH `project_id` and `project` once
                # any save_bev2_entity call touches it. An unlabeled
                # match here would DETACH DELETE those too on every
                # load_extraction call — confirmed real, not
                # hypothetical, once code-import (which calls this) also
                # writes business entities for the same project id (see
                # be-v2/docs/brownfield-code-entities-and-issue-fix-plan.md).
                tx.run(
                    "MATCH (n:Node {project: $p}) DETACH DELETE n", {"p": project},
                )
            else:
                tx.run("MATCH (n) DETACH DELETE n")
            if rows:
                tx.run(
                    "UNWIND $rows AS row CREATE (n:Node) SET n = row",
                    {"rows": rows},
                )
            if edge_rows:
                if project:
                    tx.run(
                        """
                        UNWIND $rows AS row
                        MATCH (a:Node {id: row.source, project: $p}),
                              (b:Node {id: row.target, project: $p})
                        CREATE (a)-[r:RELATES]->(b)
                        SET r = row
                        """,
                        {"rows": edge_rows, "p": project},
                    )
                else:
                    tx.run(
                        """
                        UNWIND $rows AS row
                        MATCH (a:Node {id: row.source}), (b:Node {id: row.target})
                        CREATE (a)-[r:RELATES]->(b)
                        SET r = row
                        """,
                        {"rows": edge_rows},
                    )

        def _run_tx() -> None:
            with self._driver.session() as session:
                session.execute_write(_tx)

        run_with_timeout(
            _run_tx, timeout=self._write_timeout_s,
            description=f"load_extraction (project={project or '<default>'})",
        )
        return len(rows)

    def reindex_file(self, path: str, extraction: Extraction, project: str = "") -> int:
        """Incrementally re-index one file in a single transaction.

        Deletes the file hub and its symbol nodes (DETACH DELETE also removes
        every edge touching them, including stale `calls` edges from importer
        files), inserts the fresh extraction, then re-resolves pass-2 `calls`
        edges for this file against the current graph contents and reconnects
        importer edges whose target label survived the re-parse. All reads
        happen inside the same write transaction, so the call is
        read-after-write consistent. Returns the number of nodes written.
        """
        from cie.callgraph import resolve_call_edges

        rows = self._stamped_nodes(extraction.nodes, project)
        self._maybe_compute_embeddings(rows)
        structural_edges = [dict(e) for e in extraction.edges]

        def _tx(tx) -> int:
            params = {"path": path, "p": project}
            # 1. Remember incoming `calls` edges from other files so importer
            #    edges can be reconnected to the re-parsed symbols afterwards.
            incoming = list(tx.run(
                """
                MATCH (src:Node)-[r:RELATES {relation: 'calls'}]->(t:Node)
                WHERE t.source_file = $path AND src.source_file <> $path
                  AND coalesce(src.project, '') = $p
                  AND coalesce(t.project, '') = $p
                RETURN src.id AS source, t.label AS target_label,
                       t.kind AS target_kind, r.confidence AS confidence,
                       r.line AS line
                """,
                params,
            ))
            # 2. Delete the file hub + its symbols + all their edges.
            tx.run(
                """
                MATCH (n:Node)
                WHERE n.source_file = $path AND coalesce(n.project, '') = $p
                DETACH DELETE n
                """,
                params,
            )
            # 3. Insert the fresh extraction (nodes, then structural edges).
            if rows:
                tx.run(
                    "UNWIND $rows AS row CREATE (n:Node) SET n = row",
                    {"rows": rows},
                )
            if structural_edges:
                tx.run(
                    """
                    UNWIND $rows AS row
                    MATCH (a:Node {id: row.source}), (b:Node {id: row.target})
                    WHERE coalesce(a.project, '') = $p
                      AND coalesce(b.project, '') = $p
                    CREATE (a)-[r:RELATES]->(b)
                    SET r = row
                    """,
                    {"rows": structural_edges, "p": project},
                )
            # 4. Read back the whole project (in-transaction: sees our writes)
            #    and rebuild per-file extractions for the pass-2 resolver.
            node_rows = list(tx.run(
                """
                MATCH (n:Node)
                WHERE coalesce(n.project, '') = $p
                RETURN n.id AS id, n.label AS label, n.kind AS kind,
                       n.source_file AS source_file
                """,
                params,
            ))
            edge_rows = list(tx.run(
                """
                MATCH (a:Node)-[r:RELATES]->(b:Node)
                WHERE coalesce(a.project, '') = $p
                RETURN a.id AS source, b.id AS target, r.relation AS relation
                """,
                params,
            ))
            nodes_by_file: dict[str, list[dict]] = {}
            for record in node_rows:
                data = {
                    "id": record["id"],
                    "label": record["label"] or record["id"],
                    "kind": record["kind"] or NodeKind.SYMBOL.value,
                    "source_file": record["source_file"] or "",
                }
                nodes_by_file.setdefault(data["source_file"], []).append(data)
            edges_by_file: dict[str, list[dict]] = {}
            node_file = {
                n["id"]: n["source_file"]
                for group in nodes_by_file.values()
                for n in group
            }
            for record in edge_rows:
                source_file = node_file.get(record["source"], "")
                edges_by_file.setdefault(source_file, []).append({
                    "source": record["source"],
                    "target": record["target"],
                    "relation": record["relation"] or "",
                })
            per_file: list[Extraction] = [extraction]
            for source_file, group in nodes_by_file.items():
                if source_file == path:
                    continue  # the real extraction already covers this file
                per_file.append(Extraction(
                    nodes=group, edges=edges_by_file.get(source_file, []),
                ))
            call_edges = resolve_call_edges(per_file)
            # Only this file's call sites may emit edges; guard against any
            # resolver output sourced from the DB-backed pseudo-extractions.
            fresh_sources = {n["id"] for n in extraction.nodes}
            call_edges = [e for e in call_edges if e["source"] in fresh_sources]
            # 5. Reconnect importer edges: an old incoming edge is restored
            #    when exactly one new symbol in the file carries the same
            #    label+kind as the deleted target.
            new_by_label: dict[tuple[str, str], list[str]] = {}
            for node in extraction.nodes:
                key = (node.get("label", ""), node.get("kind", ""))
                new_by_label.setdefault(key, []).append(node["id"])
            for record in incoming:
                key = (record["target_label"] or "", record["target_kind"] or "")
                candidates = new_by_label.get(key, [])
                if len(candidates) != 1:
                    continue
                call_edges.append({
                    "source": record["source"],
                    "target": candidates[0],
                    "relation": "calls",
                    "confidence": record["confidence"] or Confidence.EXTRACTED.value,
                    "line": record["line"] or 0,
                })
            # 6. Write the freshly resolved `calls` edges.
            if call_edges:
                tx.run(
                    """
                    UNWIND $rows AS row
                    MATCH (a:Node {id: row.source}), (b:Node {id: row.target})
                    WHERE coalesce(a.project, '') = $p
                      AND coalesce(b.project, '') = $p
                    CREATE (a)-[r:RELATES]->(b)
                    SET r = row
                    """,
                    {"rows": call_edges, "p": project},
                )
            return len(rows)

        def _run_tx() -> int:
            with self._driver.session() as session:
                return session.execute_write(_tx)

        return run_with_timeout(
            _run_tx, timeout=self._write_timeout_s,
            description=f"reindex_file ({path})",
        )

    # -- code-structure queries -------------------------------------------

    def get_signature(self, name: str) -> Optional[MethodSignature]:
        needle = name.lower()
        query = """
        MATCH (n:Node)
        WHERE (n.kind = $func OR n.kind = $method)
          AND (toLower(n.label) = $needle OR toLower(n.label) CONTAINS $needle)""" + self._pw("n") + """
        OPTIONAL MATCH (c:Node {kind: $class})-[:contains]->(n)
        RETURN n, c.label AS class_name
        LIMIT 1
        """
        rows = self._run(query, {
            "needle": needle,
            "func": NodeKind.FUNC.value,
            "method": NodeKind.METHOD.value,
            "class": NodeKind.CLASS.value,
        })
        if not rows:
            return None
        node = _row_to_node(rows[0]["n"])
        params = _parse_params(node.signature)
        return_type = _parse_return_type(node.signature)
        return MethodSignature(
            node=node,
            name=node.label,
            parameters=params,
            return_type=return_type,
            enclosing_class=rows[0]["class_name"] or "",
        )

    def get_methods_of_class(self, class_name: str) -> list[MethodSignature]:
        needle = class_name.lower()
        query = """
        MATCH (c:Node {kind: $class})
        WHERE (toLower(c.label) = $needle OR toLower(c.label) CONTAINS $needle)""" + self._pw("c") + """
        MATCH (c)-[:contains]->(m:Node {kind: $method})""" + self._pw_where("m") + """
        RETURN m ORDER BY m.label
        """
        rows = self._run(query, {
            "needle": needle,
            "class": NodeKind.CLASS.value,
            "method": NodeKind.METHOD.value,
        })
        out: list[MethodSignature] = []
        for r in rows:
            node = _row_to_node(r["m"])
            out.append(MethodSignature(
                node=node,
                name=node.label,
                parameters=_parse_params(node.signature),
                return_type=_parse_return_type(node.signature),
                enclosing_class=class_name,
            ))
        return out

    def discover_features(self) -> list[str]:
        """Return a human/LLM-readable list of available querying features."""
        return _FEATURE_DESCRIPTIONS

    def list_files(self) -> list[Node]:
        rows = self._run(
            "MATCH (n:Node {kind: $kind})" + self._pw_where("n")
            + " RETURN n ORDER BY n.source_file",
            {"kind": NodeKind.FILE.value},
        )
        return [_row_to_node(r["n"]) for r in rows]

    def project_graph(self) -> tuple[list[Node], list[EdgeRecord]]:
        """Every :Node plus every RELATES edge between two of them, both
        scoped to `self._project` (or the whole graph if no project
        namespace is bound — same "no project = unscoped" convention
        every other query method on this class follows via `_pw`/
        `_pw_where`). Backs GET /api/projects/{id}/code-graph
        (cie/routes.py::get_project_code_graph)."""
        node_rows = self._run("MATCH (n:Node)" + self._pw_where("n") + " RETURN n")
        nodes = [_row_to_node(r["n"]) for r in node_rows]

        edge_query = (
            "MATCH (a:Node)-[r:RELATES]->(b:Node) WHERE true"
            + self._pw("a") + self._pw("b")
            + " RETURN r AS rel, a.id AS source_id, a.label AS source_label,"
              " b.id AS target_id, b.label AS target_label"
        )
        edge_rows = self._run(edge_query)
        edges = [_row_to_edge_record(r) for r in edge_rows]
        return nodes, edges

    # -- forge tool-layer queries (Wave B) ----------------------------------

    def search_symbols(
        self, name: str, kind: str = "", file_glob: str = "", limit: int = 20
    ) -> list[SymbolMatch]:
        """Score nodes by name match and return the best `limit` matches.

        Exact (case-insensitive) label matches score 2.0, substring matches
        1.0; ordering is score DESC then label ASC for determinism. The
        `file_glob` is applied in Python with :mod:`fnmatch` on
        ``source_file`` (fnmatch's ``*``/``?`` do not map cleanly onto one
        Cypher regex, and Neo4j's fulltext index doesn't accelerate
        arbitrary-substring `CONTAINS` matching the way it would a
        tokenized search — switching to it would change match semantics,
        e.g. no longer finding "Foo" inside "getFooBar"), so a Cypher-side
        `LIMIT` alone can't just be `limit`: a glob applied AFTER the fetch
        could filter the result below `limit` even though more matches
        exist. Previously this had NO Cypher-side limit at all (fetched
        every matching node in the whole graph before any capping) —
        `fetch_limit` bounds the worst case (a generous multiplier when a
        glob is present, so the fetch:filter ratio the glob needs isn't
        starved; tight when there's no glob to filter afterward) instead of
        an unbounded scan.
        """
        needle = name.lower()
        query = (
            """
            MATCH (n:Node)
            WHERE toLower(n.label) CONTAINS $needle"""
            + self._pw("n")
        )
        params: dict = {"needle": needle}
        if kind:
            query += "\n  AND n.kind = $kind"
            params["kind"] = kind
        query += """
            RETURN n, CASE WHEN toLower(n.label) = $needle THEN 2.0 ELSE 1.0 END AS score
            ORDER BY score DESC, n.label
            LIMIT $fetch_limit
            """
        bounded_limit = max(1, int(limit))
        params["fetch_limit"] = bounded_limit * (20 if file_glob else 1)
        rows = self._run(query, params)
        matches: list[SymbolMatch] = []
        for r in rows:
            node = _row_to_node(r["n"])
            if file_glob and not fnmatch.fnmatch(node.source_file, file_glob):
                continue
            score = float(r["score"])
            confidence = (
                Confidence.EXTRACTED if score >= 2.0 else Confidence.INFERRED
            )
            matches.append(SymbolMatch(node=node, score=score, confidence=confidence))
        return matches[:bounded_limit]

    def get_callers(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Return edges from callers into `symbol`, capped at `limit`.

        Direction is taken from the stored edge: the loader materializes
        ``(caller)-[:RELATES {relation:'calls'}]->(callee)`` and stamps the
        same orientation in the edge's source/target properties, so matching
        the physical outgoing direction into the resolved symbol node is
        exactly "has an outgoing calls edge TO symbol". Each EdgeRecord
        carries the edge's confidence. Unknown symbol -> empty list.
        """
        target_id = self._resolve_symbol_id(symbol)
        if target_id is None:
            return []
        query = (
            """
            MATCH (c:Node)-[r:RELATES {relation: 'calls'}]->(t:Node {id: $tid})
            WHERE true"""
            + self._pw("c")
            + self._pw("t")
            + """
            RETURN r AS rel, c.id AS source_id, c.label AS source_label,
                   t.id AS target_id, t.label AS target_label
            ORDER BY c.label
            LIMIT $limit
            """
        )
        rows = self._run(query, {"tid": target_id, "limit": max(1, int(limit))})
        return [_row_to_edge_record(r) for r in rows]

    def get_callees(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Reverse of :meth:`get_callers` — outgoing `calls` edges FROM symbol."""
        source_id = self._resolve_symbol_id(symbol)
        if source_id is None:
            return []
        query = (
            """
            MATCH (s:Node {id: $sid})-[r:RELATES {relation: 'calls'}]->(d:Node)
            WHERE true"""
            + self._pw("s")
            + self._pw("d")
            + """
            RETURN r AS rel, s.id AS source_id, s.label AS source_label,
                   d.id AS target_id, d.label AS target_label
            ORDER BY d.label
            LIMIT $limit
            """
        )
        rows = self._run(query, {"sid": source_id, "limit": max(1, int(limit))})
        return [_row_to_edge_record(r) for r in rows]

    def get_file_skeleton(self, path: str) -> list[Node]:
        """Return class/function/method nodes for a file, ordered by line_start.

        `path` is a case-insensitive substring match on ``source_file`` so
        agents can pass either repo-relative or absolute-ish paths. Only
        signature/docstring/line metadata is returned — never bodies (the
        schema stores no bodies).
        """
        needle = path.lower()
        query = (
            """
            MATCH (n:Node)
            WHERE n.kind IN [$class, $func, $method]
              AND toLower(n.source_file) CONTAINS $needle"""
            + self._pw("n")
            + """
            RETURN n
            ORDER BY n.source_file, n.line_start, n.label
            """
        )
        rows = self._run(query, {
            "needle": needle,
            "class": NodeKind.CLASS.value,
            "func": NodeKind.FUNC.value,
            "method": NodeKind.METHOD.value,
        })
        return [_row_to_node(r["n"]) for r in rows]

    def failing_context(
        self, test_identifier: str, depth: int = 3, limit: int = 30,
        timeout_s: float = 5.0,
    ) -> list[ContextHit]:
        """BFS outward from a failing test over outgoing structural edges.

        Traverses outgoing ``calls``/``contains``/``defines`` edges up to
        `depth` hops, groups hits by graph distance (nearest first), and
        caps the result at `limit`. Each hit carries the confidence of the
        edge that reached it on the shortest path. Unknown test node ->
        empty list.

        `timeout_s`: same server-side query-timeout guard `affected_by`
        already has (see that method's docstring, P3.16) — this method has
        the identical unbounded variable-length-path shape
        (`*1..depth` with no per-hop LIMIT before the final one) but was
        missing the timeout wrapper that makes a pathologically dense
        traversal fail fast instead of running to the driver's own
        (much longer) default.
        """
        depth = max(1, min(int(depth), 6))
        start_id = self._resolve_test_node_id(test_identifier)
        if start_id is None:
            return []
        # APOC-free variable-length expansion; depth is an int literal
        # because Cypher does not parameterize path-length bounds.
        query = (
            """
            MATCH (t:Node {id: $tid})
            WHERE true"""
            + self._pw("t")
            + """
            MATCH p = (t)-[:RELATES*1..%d]->(m:Node)
            WHERE m.id <> $tid
              AND all(r IN relationships(p)
                      WHERE r.relation IN ['calls', 'contains', 'defines'])"""
            % depth
            + self._pw("m")
            + """
            WITH m, p ORDER BY length(p) ASC, m.label
            WITH m, collect(p)[0] AS shortest
            RETURN m, length(shortest) AS dist,
                   last(relationships(shortest)).confidence AS conf
            ORDER BY dist ASC, m.label
            LIMIT $limit
            """
        )
        query = Query(query, timeout=timeout_s)
        rows = self._run(query, {"tid": start_id, "limit": max(1, int(limit))})
        hits: list[ContextHit] = []
        for r in rows:
            conf_raw = r["conf"] or Confidence.EXTRACTED.value
            try:
                confidence = Confidence(conf_raw)
            except ValueError:
                confidence = Confidence.EXTRACTED
            hits.append(
                ContextHit(
                    node=_row_to_node(r["m"]),
                    distance=r["dist"],
                    confidence=confidence,
                )
            )
        return hits

    def affected_by(
        self, file_path: str, max_depth: int = 3, direction: str = "incoming",
        max_results: int = 30, timeout_s: float = 5.0,
    ) -> list[ContextHit]:
        """Blast radius (docs/forge-rebuild-plan.md's Phase 2 / WS4, ported
        from backend/src/cie/graph_interface.py's `affected_by` —
        direction flipped per review finding P1.8, default is now
        "incoming": what depends on this file, not what it depends on).

        Same BFS shape as `failing_context` (APOC-free variable-length
        expansion over `RELATES` edges, grouped by shortest-path distance)
        but seeded from every node whose `source_file` CONTAINS
        `file_path` (there may be several — a file with multiple
        functions/classes) rather than a single resolved test node, and
        with the traversal direction itself configurable.

        `timeout_s` (closes P3.16's timeout ask): passed as the query's
        `neo4j.Query.timeout` — the driver enforces this as the
        transaction's server-side timeout (`dbms.transaction.timeout`
        equivalent per-query), raising rather than letting a pathological
        deep traversal on a large graph run unbounded. `max_results`
        (clamped by the caller, `QueryEngine.affected_by`) is the other,
        cheaper half of the same guard.
        """
        max_depth = max(1, min(int(max_depth), 6))
        max_results = max(1, min(int(max_results), 50))
        needle = file_path.lower()
        arrow = "<-[:RELATES*1..%d]-" if direction == "incoming" else "-[:RELATES*1..%d]->"
        query_text = (
            """
            MATCH (f:Node)
            WHERE toLower(f.source_file) CONTAINS $needle"""
            + self._pw("f")
            + """
            WITH collect(f.id) AS seed_ids
            UNWIND seed_ids AS fid
            MATCH (f:Node {id: fid})
            MATCH p = (f)""" + (arrow % max_depth) + """(m:Node)
            WHERE NOT m.id IN seed_ids
              AND all(r IN relationships(p)
                      WHERE r.relation IN ['calls', 'contains', 'defines', 'imports'])"""
            + self._pw("m")
            + """
            WITH m, p ORDER BY length(p) ASC, m.label
            WITH m, collect(p)[0] AS shortest
            RETURN m, length(shortest) AS dist,
                   last(relationships(shortest)).confidence AS conf
            ORDER BY dist ASC, m.label
            LIMIT $limit
            """
        )
        query = Query(query_text, timeout=timeout_s)
        rows = self._run(query, {"needle": needle, "limit": max_results})
        hits: list[ContextHit] = []
        for r in rows:
            conf_raw = r["conf"] or Confidence.EXTRACTED.value
            try:
                confidence = Confidence(conf_raw)
            except ValueError:
                confidence = Confidence.EXTRACTED
            hits.append(ContextHit(node=_row_to_node(r["m"]), distance=r["dist"], confidence=confidence))
        return hits

    def health_details(self) -> dict:
        """Connectivity + counts; raises when the database is unreachable."""
        stats = self.stats()  # any driver error propagates to the caller
        return {
            "store": "reachable",
            "nodes": stats.nodes,
            "edges": stats.edges,
            "communities": stats.communities,
            "project": self._project,
        }

    def semantic_search(
        self, query: str, top_k: int = 10, project: str = ""
    ) -> list[SemanticMatch]:
        """Embed `query` and rank nodes by cosine similarity via Neo4j's
        native vector index (`node_embedding_idx`). Empty list when the
        query fails to embed (e.g. the embeddings API is unreachable) or
        when nothing in scope has an embedding yet — this never raises for
        "no matches", matching the other Wave-B not-found conventions.

        `project` optionally overrides the repository's own project scope
        (`self._project`) for this one call; empty (the default) uses
        whatever the repository was constructed with.
        """
        if not query:
            return []
        try:
            query_vector = embed_text(query, input_type="query")
        except Exception:  # noqa: BLE001 - degrade to "no matches", don't raise
            logger.warning("semantic_search: failed to embed query", exc_info=True)
            return []
        if not query_vector:
            return []
        effective_project = project or self._project
        cypher = """
        CALL db.index.vector.queryNodes('node_embedding_idx', $top_k, $queryVector)
        YIELD node, score
        WHERE node.embedding IS NOT NULL
        """
        if effective_project:
            cypher += "  AND node.project = $project\n"
        cypher += """
        RETURN node, score
        ORDER BY score DESC
        LIMIT $top_k
        """
        rows = self._run(cypher, {
            "top_k": max(1, int(top_k)),
            "queryVector": list(query_vector),
            "project": effective_project,
        })
        return [
            SemanticMatch(node=_row_to_node(dict(r["node"])), score=r["score"])
            for r in rows
        ]

    def _resolve_symbol_id(self, symbol: str) -> Optional[str]:
        """Resolve a symbol name to a node id, or None when unknown.

        Exact label matches win over substring matches; function/method
        kinds are preferred over classes/files at equal name rank.
        """
        needle = symbol.lower()
        query = (
            """
            MATCH (n:Node)
            WHERE toLower(n.label) CONTAINS $needle"""
            + self._pw("n")
            + """
            WITH n,
                 CASE WHEN toLower(n.label) = $needle THEN 0 ELSE 1 END AS name_rank,
                 CASE WHEN n.kind IN [$func, $method] THEN 0 ELSE 1 END AS kind_rank
            RETURN n.id AS id
            ORDER BY name_rank, kind_rank, n.label
            LIMIT 1
            """
        )
        rows = self._run(query, {
            "needle": needle,
            "func": NodeKind.FUNC.value,
            "method": NodeKind.METHOD.value,
        })
        return rows[0]["id"] if rows else None

    def _resolve_test_node_id(self, test_identifier: str) -> Optional[str]:
        """Map a pytest-style identifier to its graph node id, or None.

        ``"path/to/test_file.py::test_name"`` resolves to the node whose
        label contains ``test_name`` (preferring exact labels, function/
        method kinds, and nodes in the file before ``::``). A bare path
        resolves to the matching file node (any node in that file as a
        fallback so BFS still has a start).
        """
        if "::" in test_identifier:
            file_part, _, func_name = test_identifier.rpartition("::")
            needle = func_name.strip().lower()
            if not needle:
                return None
            query = (
                """
                MATCH (n:Node)
                WHERE toLower(n.label) CONTAINS $needle"""
                + self._pw("n")
                + """
                WITH n,
                     CASE WHEN toLower(n.label) = $needle THEN 0 ELSE 1 END AS name_rank,
                     CASE WHEN n.kind IN [$func, $method] THEN 0 ELSE 1 END AS kind_rank,
                     CASE WHEN $file <> '' AND toLower(n.source_file) CONTAINS $file
                          THEN 0 ELSE 1 END AS file_rank
                RETURN n.id AS id
                ORDER BY name_rank, kind_rank, file_rank, n.label
                LIMIT 1
                """
            )
            rows = self._run(query, {
                "needle": needle,
                "file": file_part.strip().lower(),
                "func": NodeKind.FUNC.value,
                "method": NodeKind.METHOD.value,
            })
            return rows[0]["id"] if rows else None
        needle = test_identifier.strip().lower()
        if not needle:
            return None
        query = (
            """
            MATCH (n:Node)
            WHERE toLower(n.source_file) CONTAINS $needle"""
            + self._pw("n")
            + """
            WITH n, CASE WHEN n.kind = $file_kind THEN 0 ELSE 1 END AS kind_rank,
                 CASE WHEN $needle CONTAINS toLower(n.label) THEN 0 ELSE 1 END AS label_rank
            RETURN n.id AS id
            ORDER BY kind_rank, label_rank, n.source_file
            LIMIT 1
            """
        )
        rows = self._run(query, {
            "needle": needle,
            "file_kind": NodeKind.FILE.value,
        })
        return rows[0]["id"] if rows else None

    # -- QA coverage (be-v2/docs/design/qa-persona-cie-knowledge-graph.md) --
    #
    # `record_coverage`/`get_coverage` deliberately use an EXACT match on
    # `source_file`, not the CONTAINS-substring convention every read-only
    # query method above uses (get_file_skeleton, affected_by, etc.).
    # Those are reads, where a slightly-loose match just returns extra
    # results; this is a WRITE, where CONTAINS matching more than one file
    # would silently attach a measurement to the wrong node. Callers must
    # pass the same repo-relative path the coverage tool reported.

    def _derive_function_coverage(
        self, file_path: str, uncovered: list[int]
    ) -> list[FunctionCoverage]:
        """Intersects `uncovered` against every FUNC/METHOD node's known
        line range for `file_path` and stamps each node's own
        `coverage_pct` — no separate measurement, just arithmetic over
        line ranges AST extraction already recorded."""
        query = (
            """
            MATCH (n:Node {source_file: $file_path})
            WHERE n.kind IN [$func, $method]"""
            + self._pw("n")
            + """
            RETURN n.id AS id, n.label AS label,
                   n.line_start AS line_start, n.line_end AS line_end
            ORDER BY n.line_start
            """
        )
        rows = self._run(query, {
            "file_path": file_path,
            "func": NodeKind.FUNC.value,
            "method": NodeKind.METHOD.value,
        })
        uncovered_set = set(uncovered)
        functions: list[FunctionCoverage] = []
        updates: list[dict] = []
        for r in rows:
            line_start, line_end = r["line_start"] or 0, r["line_end"] or 0
            if line_end < line_start:
                pct = 100.0
            else:
                total = line_end - line_start + 1
                miss = sum(1 for ln in range(line_start, line_end + 1) if ln in uncovered_set)
                pct = round((1 - miss / total) * 100, 1)
            functions.append(FunctionCoverage(
                label=r["label"], line_start=line_start, line_end=line_end,
                coverage_pct=pct,
            ))
            updates.append({"id": r["id"], "pct": pct})
        if updates:
            self._run(
                "UNWIND $rows AS row MATCH (n:Node {id: row.id}) "
                "SET n.coverage_pct = row.pct",
                {"rows": updates},
            )
        return functions

    def record_coverage(
        self,
        file_path: str,
        subtree: str,
        coverage_pct: float,
        covered_lines: int,
        uncovered_lines: Sequence[int],
        measured_at: str,
    ) -> Optional[FileCoverage]:
        uncovered = sorted({int(n) for n in uncovered_lines})
        query = (
            """
            MATCH (n:Node {source_file: $file_path, kind: $file_kind})
            WHERE true"""
            + self._pw("n")
            + """
            SET n.coverage_pct = $coverage_pct,
                n.covered_lines = $covered_lines,
                n.uncovered_line_count = $uncovered_line_count,
                n.uncovered_lines = $uncovered_lines,
                n.coverage_measured_at = $measured_at,
                n.coverage_subtree = $subtree
            RETURN n
            """
        )
        rows = self._run(query, {
            "file_path": file_path,
            "file_kind": NodeKind.FILE.value,
            "coverage_pct": float(coverage_pct),
            "covered_lines": int(covered_lines),
            "uncovered_line_count": len(uncovered),
            "uncovered_lines": uncovered,
            "measured_at": measured_at,
            "subtree": subtree,
        })
        if not rows:
            return None
        functions = self._derive_function_coverage(file_path, uncovered)
        return FileCoverage(
            file_path=file_path,
            coverage_pct=float(coverage_pct),
            covered_lines=int(covered_lines),
            uncovered_line_count=len(uncovered),
            uncovered_lines=tuple(uncovered),
            measured_at=measured_at,
            subtree=subtree,
            functions=tuple(functions),
        )

    def get_coverage(self, file_path: str, subtree: str = "") -> Optional[FileCoverage]:
        query = (
            """
            MATCH (n:Node {source_file: $file_path, kind: $file_kind})
            WHERE n.coverage_pct IS NOT NULL"""
            + self._pw("n")
            + (" AND n.coverage_subtree = $subtree" if subtree else "")
            + """
            RETURN n
            LIMIT 1
            """
        )
        rows = self._run(query, {
            "file_path": file_path,
            "file_kind": NodeKind.FILE.value,
            "subtree": subtree,
        })
        if not rows:
            return None
        n = rows[0]["n"]
        func_query = (
            """
            MATCH (n:Node {source_file: $file_path})
            WHERE n.kind IN [$func, $method] AND n.coverage_pct IS NOT NULL"""
            + self._pw("n")
            + """
            RETURN n.label AS label, n.line_start AS line_start,
                   n.line_end AS line_end, n.coverage_pct AS coverage_pct
            ORDER BY n.line_start
            """
        )
        func_rows = self._run(func_query, {
            "file_path": file_path,
            "func": NodeKind.FUNC.value,
            "method": NodeKind.METHOD.value,
        })
        functions = [
            FunctionCoverage(
                label=r["label"], line_start=r["line_start"] or 0,
                line_end=r["line_end"] or 0, coverage_pct=r["coverage_pct"],
            )
            for r in func_rows
        ]
        return FileCoverage(
            file_path=n.get("source_file") or file_path,
            coverage_pct=n.get("coverage_pct"),
            covered_lines=n.get("covered_lines") or 0,
            uncovered_line_count=n.get("uncovered_line_count") or 0,
            uncovered_lines=tuple(n.get("uncovered_lines") or []),
            measured_at=n.get("coverage_measured_at") or "",
            subtree=n.get("coverage_subtree") or "",
            functions=tuple(functions),
        )

    def coverage_report(
        self,
        subtree: str = "",
        below_pct: Optional[float] = None,
        file_glob: str = "",
        include_unmeasured: bool = True,
    ) -> list[FileCoverageSummary]:
        query = (
            """
            MATCH (n:Node {kind: $file_kind})
            WHERE true"""
            + self._pw("n")
            + (" AND n.coverage_subtree = $subtree" if subtree else "")
            + """
            RETURN n.source_file AS file_path, n.coverage_pct AS coverage_pct,
                   n.uncovered_line_count AS uncovered_line_count,
                   n.coverage_measured_at AS measured_at,
                   n.coverage_subtree AS subtree
            """
        )
        rows = self._run(query, {"file_kind": NodeKind.FILE.value, "subtree": subtree})
        out: list[FileCoverageSummary] = []
        for r in rows:
            fp = r["file_path"] or ""
            if file_glob and not fnmatch.fnmatch(fp, file_glob):
                continue
            pct = r["coverage_pct"]
            measured = pct is not None
            if not measured and not include_unmeasured:
                continue
            if measured and below_pct is not None and pct >= below_pct:
                continue
            out.append(FileCoverageSummary(
                file_path=fp, measured=measured, coverage_pct=pct,
                uncovered_line_count=r["uncovered_line_count"] or 0,
                measured_at=r["measured_at"] or "", subtree=r["subtree"] or "",
            ))
        out.sort(key=lambda s: s.coverage_pct if s.coverage_pct is not None else -1.0)
        return out

    def record_coverage_snapshot(self, snapshot: CoverageSnapshot) -> None:
        query = """
        CREATE (s:CoverageSnapshot {
            project: $project, subtree: $subtree, aggregate_pct: $aggregate_pct,
            files_measured: $files_measured, total_lines: $total_lines,
            covered_lines: $covered_lines, measured_at: $measured_at
        })
        """
        self._run(query, {
            "subtree": snapshot.subtree,
            "aggregate_pct": float(snapshot.aggregate_pct),
            "files_measured": int(snapshot.files_measured),
            "total_lines": int(snapshot.total_lines),
            "covered_lines": int(snapshot.covered_lines),
            "measured_at": snapshot.measured_at,
        })

    def coverage_trend(
        self, subtree: str = "", limit: int = 20
    ) -> list[CoverageSnapshot]:
        query = (
            "MATCH (s:CoverageSnapshot {project: $project})"
            + (" WHERE s.subtree = $subtree" if subtree else "")
            + " RETURN s ORDER BY s.measured_at DESC LIMIT $limit"
        )
        rows = self._run(query, {"subtree": subtree, "limit": int(limit)})
        out: list[CoverageSnapshot] = []
        for r in rows:
            s = r["s"]
            out.append(CoverageSnapshot(
                project=self._project,
                subtree=s.get("subtree") or "",
                aggregate_pct=s.get("aggregate_pct") or 0.0,
                files_measured=s.get("files_measured") or 0,
                total_lines=s.get("total_lines") or 0,
                covered_lines=s.get("covered_lines") or 0,
                measured_at=s.get("measured_at") or "",
            ))
        return out
