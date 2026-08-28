"""An in-memory Repository — both a test double AND the query/traversal
engine `cie.embedded_repository.EmbeddedRepository` (Phase 0 of
docs/growth-plan.md, the zero-config graph backend) wraps for real,
persistent, no-Neo4j-required use.

Implements the `cie.repository.Repository` protocol over plain Python
dicts. The traversal and shortest-path implementations mirror the
semantics of `Neo4jRepository`'s own Cypher queries — this class was
already the reference test double both are verified against, before this
file existed as anything other than a test fixture.

Imports `embed_text` from `cie.embed` (not `core.llm.embed_text`
directly) — same lazy, overridable shim `cie.neo4j_repository` uses (see
`cie.embed`'s own module docstring) — so this module, like the rest of
cie, imports cleanly with no `core.llm` on the path.
"""

from __future__ import annotations

import dataclasses
import fnmatch
from collections import defaultdict, deque
from datetime import datetime, timezone
from typing import Optional, Sequence

from cie.embed import cosine_similarity, embed_text
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
    HybridMatch,
    MethodSignature,
    MetricSnapshot,
    Node,
    NodeKind,
    NodeRecord,
    PathResult,
    SemanticMatch,
    SymbolMatch,
    TraversalMode,
    TraversalResult,
)


class InMemoryRepository:
    def __init__(self, nodes: list[Node], edges: list[Edge]):
        self._nodes: dict[str, Node] = {n.id: n for n in nodes}
        self._edges: list[Edge] = edges
        self._file_coverage: dict[str, dict] = {}
        self._func_coverage: dict[str, float] = {}
        self._snapshots: list[CoverageSnapshot] = []
        self._metric_snapshots: list[MetricSnapshot] = []
        self._telemetry_events: list[dict] = []
        self._telemetry_seq: dict[tuple[str, str], int] = {}
        self._adj: dict[str, list[tuple[str, Edge]]] = defaultdict(list)
        self._rebuild_adjacency()

    def _rebuild_adjacency(self) -> None:
        self._adj = defaultdict(list)
        for e in self._edges:
            self._adj[e.source].append((e.target, e))
            self._adj[e.target].append((e.source, e))

    def _label(self, nid: str) -> str:
        return self._nodes[nid].label if nid in self._nodes else nid

    def _resolve_symbol_id(self, symbol: str) -> Optional[str]:
        """Exact label matches win over substrings; func/method beat other
        kinds. Picks exactly ONE node — safe for a single best-guess
        lookup, but NOT for "every edge touching this name": see
        `_resolve_symbol_ids` below for the ambiguous-name fix
        (`get_callers`/`get_callees`/`test_map`/`actual_callers` all use
        that instead, not this)."""
        needle = symbol.lower()
        candidates = [
            (
                0 if n.label.lower() == needle else 1,
                0 if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value) else 1,
                n.label,
                nid,
            )
            for nid, n in self._nodes.items()
            if needle in n.label.lower()
        ]
        if not candidates:
            return None
        candidates.sort()
        return candidates[0][3]

    def _resolve_symbol_ids(self, symbol: str) -> list[str]:
        """Every node whose label EXACTLY matches `symbol` (case-
        insensitive) — the ambiguous-name fix from docs/
        competitor-benchmarks.md's Task 2 (found 2026-08-28, fixed here):
        a bare name can legitimately name multiple distinct definitions
        (two different classes each with their own same-named method),
        and `get_callers`/`get_callees`/`test_map`/`actual_callers` need
        to see edges from ALL of them — `_resolve_symbol_id`'s single
        pick silently returned only whichever one node id happened to
        rank first, a confident-looking but incomplete result (verified
        against real grep ground truth: 126 real call sites split across
        two `_post` methods, only 2 of which `_resolve_symbol_id`-based
        lookup surfaced).

        `cie.callgraph.resolve_call_edges` itself was already verified
        correct in that investigation (all 126/126 real call sites
        resolved to the right one of the two targets) — this fixes the
        bare-name lookup layer above it, not the call-graph resolver.

        Falls back to `_resolve_symbol_id`'s single substring-match
        result when there is no EXACT match at all, so an unambiguous
        name's behavior — the overwhelmingly common case — is unchanged.
        """
        needle = symbol.lower()
        exact = [nid for nid, n in self._nodes.items() if n.label.lower() == needle]
        if exact:
            exact.sort(key=lambda nid: (
                0 if self._nodes[nid].kind in (NodeKind.FUNC.value, NodeKind.METHOD.value) else 1,
                self._nodes[nid].label,
                nid,
            ))
            return exact
        single = self._resolve_symbol_id(symbol)
        return [single] if single is not None else []

    def _resolve_test_node_id(self, test_identifier: str) -> Optional[str]:
        """Mirrors Neo4jRepository._resolve_test_node_id's ranking rules."""
        if "::" in test_identifier:
            file_part, _, func_name = test_identifier.rpartition("::")
            needle = func_name.strip().lower()
            if not needle:
                return None
            file_needle = file_part.strip().lower()
            candidates = [
                (
                    0 if n.label.lower() == needle else 1,
                    0 if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value) else 1,
                    0 if file_needle and file_needle in n.source_file.lower() else 1,
                    n.label,
                    nid,
                )
                for nid, n in self._nodes.items()
                if needle in n.label.lower()
            ]
            if not candidates:
                return None
            candidates.sort()
            return candidates[0][4]
        needle = test_identifier.strip().lower()
        if not needle:
            return None
        candidates = [
            (
                0 if n.kind == NodeKind.FILE.value else 1,
                0 if n.label.lower() in needle else 1,
                n.source_file,
                nid,
            )
            for nid, n in self._nodes.items()
            if needle in n.source_file.lower()
        ]
        if not candidates:
            return None
        candidates.sort()
        return candidates[0][3]

    def find_nodes_by_terms(self, terms: Sequence[str]) -> list[tuple[float, Node]]:
        lowered = [t.lower() for t in terms]
        scored: list[tuple[float, Node]] = []
        for n in self._nodes.values():
            label = n.label.lower()
            src = n.source_file.lower()
            score = sum(1.0 for t in lowered if t in label) + sum(
                0.5 for t in lowered if t in src
            )
            if score > 0:
                scored.append((score, n))
        return sorted(scored, key=lambda kv: kv[0], reverse=True)

    def get_node(self, label_or_id: str) -> Optional[NodeRecord]:
        needle = label_or_id.lower()
        for nid, n in self._nodes.items():
            if needle in n.label.lower() or needle == nid.lower():
                return NodeRecord(node=n, degree=len(self._adj.get(nid, [])))
        return None

    def get_neighbors(
        self, label_or_id: str, relation_filter: str = ""
    ) -> Optional[list[EdgeRecord]]:
        needle = label_or_id.lower()
        nid: Optional[str] = None
        for candidate_id, n in self._nodes.items():
            if needle in n.label.lower() or needle == candidate_id.lower():
                nid = candidate_id
                break
        if nid is None:
            return None
        out: list[EdgeRecord] = []
        rfilter = relation_filter.lower()
        for other, e in self._adj.get(nid, []):
            if rfilter and rfilter not in e.relation.lower():
                continue
            out.append(
                EdgeRecord(
                    edge=e,
                    source_label=self._label(e.source),
                    target_label=self._label(e.target),
                )
            )
        return out

    def get_community(self, community_id: int) -> list[Node]:
        return [
            n
            for n in self._nodes.values()
            if n.community == community_id
        ]

    def list_communities(self) -> list[tuple[int, int]]:
        counts: dict[int, int] = defaultdict(int)
        for n in self._nodes.values():
            if n.community is not None:
                counts[n.community] += 1
        return sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))

    def god_nodes(self, top_n: int = 10) -> list[NodeRecord]:
        degrees = [
            (nid, len(self._adj.get(nid, [])))
            for nid in self._nodes
            if len(self._adj.get(nid, [])) > 0
        ]
        degrees.sort(key=lambda kv: kv[1], reverse=True)
        out: list[NodeRecord] = []
        for nid, deg in degrees[:top_n]:
            n = self._nodes[nid]
            if n.label.split(".")[-1] in (
                "py", "ts", "js", "go", "rs", "java", "rb", "cpp", "c", "h"
            ):
                continue
            if n.label.startswith(".") and n.label.endswith("()"):
                continue
            out.append(NodeRecord(node=n, degree=deg))
        return out[:top_n]

    def stats(self) -> GraphStats:
        conf_counts: dict[str, int] = defaultdict(int)
        for e in self._edges:
            conf_counts[e.confidence.value] += 1
        communities = {n.community for n in self._nodes.values() if n.community is not None}
        return GraphStats(
            nodes=len(self._nodes),
            edges=len(self._edges),
            communities=len(communities),
            confidence_counts=dict(conf_counts),
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
        # BFS shortest path.
        prev: dict[str, tuple[str, Edge]] = {}
        visited = {src_id}
        q = deque([src_id])
        while q:
            cur = q.popleft()
            if cur == tgt_id:
                break
            for other, e in self._adj.get(cur, []):
                if other not in visited:
                    visited.add(other)
                    prev[other] = (cur, e)
                    q.append(other)
        if tgt_id not in prev and src_id != tgt_id:
            return None
        # Reconstruct.
        path_ids: list[str] = [tgt_id]
        path_edges: list[Edge] = []
        cur = tgt_id
        while cur != src_id:
            p, e = prev[cur]
            path_ids.append(p)
            path_edges.append(e)
            cur = p
        path_ids.reverse()
        path_edges.reverse()
        hops = len(path_edges)
        if hops > max_hops:
            return None
        nodes = tuple(self._nodes[i] for i in path_ids)
        edges = tuple(
            EdgeRecord(
                edge=e,
                source_label=self._label(e.source),
                target_label=self._label(e.target),
            )
            for e in path_edges
        )
        return PathResult(hops=hops, nodes=nodes, edges=edges)

    def traverse(
        self,
        start_labels: Sequence[str],
        mode: TraversalMode,
        depth: int,
    ) -> TraversalResult:
        depth = max(1, min(int(depth), 6))
        start_ids: list[str] = []
        resolved_labels: list[str] = []
        for label in start_labels:
            scored = self.find_nodes_by_terms(label.split())
            if scored:
                start_ids.append(scored[0][1].id)
                resolved_labels.append(scored[0][1].label)
        if not start_ids:
            return TraversalResult(
                mode=mode, depth=depth, start_labels=tuple(start_labels),
                nodes=(), edges=(),
            )
        visited: set[str] = set(start_ids)
        seen_edges: dict[tuple[str, str, str], EdgeRecord] = {}
        if mode == TraversalMode.BFS:
            frontier = list(start_ids)
            for _ in range(depth):
                nxt: list[str] = []
                for n in frontier:
                    for other, e in self._adj.get(n, []):
                        if other not in visited:
                            visited.add(other)
                            nxt.append(other)
                            seen_edges.setdefault(
                                (e.source, e.target, e.relation),
                                EdgeRecord(
                                    edge=e,
                                    source_label=self._label(e.source),
                                    target_label=self._label(e.target),
                                ),
                            )
                frontier = nxt
        else:
            stack = list(start_ids)
            while stack:
                n = stack.pop()
                for other, e in self._adj.get(n, []):
                    if other not in visited:
                        visited.add(other)
                        stack.append(other)
                        seen_edges.setdefault(
                            (e.source, e.target, e.relation),
                            EdgeRecord(
                                edge=e,
                                source_label=self._label(e.source),
                                target_label=self._label(e.target),
                            ),
                        )
        nodes = tuple(self._nodes[i] for i in visited if i in self._nodes)
        return TraversalResult(
            mode=mode,
            depth=depth,
            start_labels=tuple(resolved_labels),
            nodes=nodes,
            edges=tuple(seen_edges.values()),
        )

    # -- loader -----------------------------------------------------------

    def load_extraction(self, nodes, edges, project: str = "") -> int:
        self._nodes = {n["id"]: _dict_to_node(n) for n in nodes}
        self._edges = [_dict_to_edge(e) for e in edges]
        self._rebuild_adjacency()
        return len(self._nodes)

    # -- code-structure queries -------------------------------------------

    def get_signature(self, name: str) -> Optional[MethodSignature]:
        needle = name.lower()
        for n in self._nodes.values():
            if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value):
                if needle == n.label.lower() or needle in n.label.lower():
                    enclosing = self._enclosing_class(n.id)
                    return MethodSignature(
                        node=n,
                        name=n.label,
                        parameters=_parse_params(n.signature),
                        return_type=_parse_return_type(n.signature),
                        enclosing_class=enclosing,
                    )
        return None

    def get_methods_of_class(self, class_name: str) -> list[MethodSignature]:
        needle = class_name.lower()
        class_ids = [
            nid for nid, n in self._nodes.items()
            if n.kind == NodeKind.CLASS.value
            and (needle == n.label.lower() or needle in n.label.lower())
        ]
        out: list[MethodSignature] = []
        for cid in class_ids:
            for other, e in self._adj.get(cid, []):
                if e.source == cid and e.relation == "contains":
                    m = self._nodes[other]
                    if m.kind == NodeKind.METHOD.value:
                        out.append(MethodSignature(
                            node=m,
                            name=m.label,
                            parameters=_parse_params(m.signature),
                            return_type=_parse_return_type(m.signature),
                            enclosing_class=self._nodes[cid].label,
                        ))
        return out

    def discover_features(self) -> list[str]:
        from cie.neo4j_repository import _FEATURE_DESCRIPTIONS
        return _FEATURE_DESCRIPTIONS

    def list_files(self) -> list[Node]:
        return [
            n for n in self._nodes.values()
            if n.kind == NodeKind.FILE.value
        ]

    def project_graph(self) -> tuple[list[Node], list[EdgeRecord]]:
        return (
            list(self._nodes.values()),
            [
                EdgeRecord(edge=e, source_label=self._label(e.source), target_label=self._label(e.target))
                for e in self._edges
            ],
        )

    def _enclosing_class(self, method_id: str) -> str:
        for other, e in self._adj.get(method_id, []):
            if e.target == method_id and e.relation == "contains":
                cls = self._nodes.get(e.source)
                if cls and cls.kind == NodeKind.CLASS.value:
                    return cls.label
        return ""

    def get_callers(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        target_ids = self._resolve_symbol_ids(symbol)
        if not target_ids:
            return []
        out = [
            EdgeRecord(edge=e, source_label=self._label(e.source), target_label=self._label(e.target))
            for target_id in target_ids
            for _, e in self._adj.get(target_id, [])
            if e.relation == "calls" and e.target == target_id
        ]
        out.sort(key=lambda er: er.source_label)
        return out[: max(1, int(limit))]

    def get_callees(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        source_ids = self._resolve_symbol_ids(symbol)
        if not source_ids:
            return []
        out = [
            EdgeRecord(edge=e, source_label=self._label(e.source), target_label=self._label(e.target))
            for source_id in source_ids
            for _, e in self._adj.get(source_id, [])
            if e.relation == "calls" and e.source == source_id
        ]
        out.sort(key=lambda er: er.target_label)
        return out[: max(1, int(limit))]

    def search_symbols(
        self, name: str, kind: str = "", file_glob: str = "", limit: int = 20
    ) -> list[SymbolMatch]:
        needle = name.lower()
        matches: list[SymbolMatch] = []
        for n in self._nodes.values():
            if needle not in n.label.lower():
                continue
            if n.properties.get("status") == "DEPRECATED":
                continue
            if kind and n.kind != kind:
                continue
            if file_glob and not fnmatch.fnmatch(n.source_file, file_glob):
                continue
            score = 2.0 if n.label.lower() == needle else 1.0
            confidence = Confidence.EXTRACTED if score >= 2.0 else Confidence.INFERRED
            matches.append(SymbolMatch(node=n, score=score, confidence=confidence))
        matches.sort(key=lambda m: (-m.score, m.node.label))
        return matches[: max(1, int(limit))]

    def get_file_skeleton(self, path: str) -> list[Node]:
        needle = path.lower()
        matches = [
            n
            for n in self._nodes.values()
            if n.kind in (NodeKind.CLASS.value, NodeKind.FUNC.value, NodeKind.METHOD.value)
            and needle in n.source_file.lower()
        ]
        matches.sort(key=lambda n: (n.source_file, n.line_start, n.label))
        return matches

    def failing_context(
        self, test_identifier: str, depth: int = 3, limit: int = 30
    ) -> list[ContextHit]:
        depth = max(1, min(int(depth), 6))
        start_id = self._resolve_test_node_id(test_identifier)
        if start_id is None:
            return []
        allowed = {"calls", "contains", "defines"}
        best_dist: dict[str, int] = {}
        best_conf: dict[str, Confidence] = {}
        frontier = {start_id}
        visited = {start_id}
        hop = 0
        while frontier and hop < depth:
            hop += 1
            nxt: set[str] = set()
            for cur in frontier:
                for other, e in self._adj.get(cur, []):
                    if e.source != cur or e.relation not in allowed or other in visited:
                        continue
                    visited.add(other)
                    best_dist[other] = hop
                    best_conf[other] = e.confidence
                    nxt.add(other)
            frontier = nxt
        hits = [
            ContextHit(node=self._nodes[nid], distance=d, confidence=best_conf[nid])
            for nid, d in best_dist.items()
        ]
        hits.sort(key=lambda h: (h.distance, h.node.label))
        return hits[: max(1, int(limit))]

    def affected_by(
        self,
        file_path: str,
        max_depth: int = 3,
        direction: str = "incoming",
        max_results: int = 30,
    ) -> list[ContextHit]:
        max_depth = max(1, min(int(max_depth), 6))
        max_results = max(1, min(int(max_results), 50))
        needle = file_path.lower()
        seed_ids = {nid for nid, n in self._nodes.items() if needle in n.source_file.lower()}
        if not seed_ids:
            return []
        allowed = {"calls", "contains", "defines", "imports"}
        best_dist: dict[str, int] = {}
        best_conf: dict[str, Confidence] = {}
        frontier = set(seed_ids)
        visited = set(seed_ids)
        hop = 0
        while frontier and hop < max_depth:
            hop += 1
            nxt: set[str] = set()
            for cur in frontier:
                for other, e in self._adj.get(cur, []):
                    if e.relation not in allowed:
                        continue
                    if direction == "incoming":
                        if e.target != cur:
                            continue
                        neighbor = e.source
                    else:
                        if e.source != cur:
                            continue
                        neighbor = e.target
                    if neighbor in visited:
                        continue
                    visited.add(neighbor)
                    best_dist[neighbor] = hop
                    best_conf[neighbor] = e.confidence
                    nxt.add(neighbor)
            frontier = nxt
        hits = [
            ContextHit(node=self._nodes[nid], distance=d, confidence=best_conf[nid])
            for nid, d in best_dist.items()
        ]
        hits.sort(key=lambda h: (h.distance, h.node.label))
        return hits[:max_results]

    def get_file_symbols(self, path: str) -> list[Node]:
        file_nodes = [n for n in self._nodes.values() if n.id == path or n.source_file == path]
        if not file_nodes:
            return []
        out: dict[str, Node] = {}
        for f in file_nodes:
            out[f.id] = f
            for other, e in self._adj.get(f.id, []):
                if e.relation in ("defines", "contains"):
                    out[other] = self._nodes[other]
        return list(out.values())

    def reindex_file(self, path: str, extraction, project: str = "") -> int:
        """Incrementally re-index one file; mirrors Neo4jRepository.reindex_file.

        Drops the file's existing nodes/edges, inserts the fresh extraction,
        then re-resolves `calls` edges using the same per-file reconstruction
        + reconnection strategy as the Neo4j implementation (see there for
        why: other files only contribute their symbol index, not their
        original imports/call_sites, to the pass-2 resolver).
        """
        from cie.callgraph import resolve_call_edges
        from cie.extract import Extraction

        stale_ids = {nid for nid, n in self._nodes.items() if n.source_file == path}
        # Remember incoming `calls` edges from other files, keyed by the
        # deleted target's (label, kind), so they can be reconnected to
        # whichever fresh symbol replaces that target below.
        incoming = [
            (e.source, self._nodes[e.target].label, self._nodes[e.target].kind, e.confidence)
            for e in self._edges
            if e.relation == "calls" and e.target in stale_ids and e.source not in stale_ids
        ]
        for nid in stale_ids:
            del self._nodes[nid]
        self._edges = [
            e for e in self._edges if e.source not in stale_ids and e.target not in stale_ids
        ]

        new_nodes = [_dict_to_node(n) for n in extraction.nodes]
        for n in new_nodes:
            self._nodes[n.id] = n
        self._edges.extend(_dict_to_edge(e) for e in extraction.edges)

        nodes_by_file: dict[str, list[dict]] = defaultdict(list)
        for n in self._nodes.values():
            nodes_by_file[n.source_file].append(
                {"id": n.id, "label": n.label, "kind": n.kind, "source_file": n.source_file}
            )
        node_file = {n.id: n.source_file for n in self._nodes.values()}
        edges_by_file: dict[str, list[dict]] = defaultdict(list)
        for e in self._edges:
            edges_by_file[node_file.get(e.source, "")].append(
                {"source": e.source, "target": e.target, "relation": e.relation}
            )
        per_file = [extraction]
        for source_file, group in nodes_by_file.items():
            if source_file == path:
                continue
            per_file.append(Extraction(nodes=group, edges=edges_by_file.get(source_file, [])))
        call_edges = resolve_call_edges(per_file)
        fresh_sources = {n["id"] for n in extraction.nodes}
        call_edges = [e for e in call_edges if e["source"] in fresh_sources]

        new_by_label: dict[tuple[str, str], list[str]] = defaultdict(list)
        for node in extraction.nodes:
            new_by_label[(node.get("label", ""), node.get("kind", ""))].append(node["id"])
        for source_id, target_label, target_kind, confidence in incoming:
            candidates = new_by_label.get((target_label, target_kind), [])
            if len(candidates) != 1:
                continue
            call_edges.append({
                "source": source_id,
                "target": candidates[0],
                "relation": "calls",
                "confidence": confidence.value,
            })

        self._edges.extend(_dict_to_edge(e) for e in call_edges)
        self._rebuild_adjacency()
        return len(new_nodes)

    # Mirrors Neo4jRepository.hybrid_search's weights exactly (see there
    # for the reasoning) — kept as a separate constant here rather than
    # importing from neo4j_repository, so this fake has zero Neo4j import
    # dependency, same as the rest of this file.
    _HYBRID_LEXICAL_WEIGHT = 0.45
    _HYBRID_DENSE_WEIGHT = 0.45
    _HYBRID_GRAPH_WEIGHT = 0.10

    @staticmethod
    def _hybrid_normalized(raw: dict[str, float]) -> dict[str, float]:
        if not raw:
            return {}
        peak = max(raw.values())
        if peak <= 0:
            return {k: 0.0 for k in raw}
        return {k: v / peak for k, v in raw.items()}

    def hybrid_search(
        self, query: str, top_k: int = 10, project: str = "",
    ) -> list[HybridMatch]:
        """Mirrors Neo4jRepository.hybrid_search's three-signal combination
        in pure Python: lexical = term-in-label count, dense = cosine
        similarity against whatever `node.embedding` a fixture node
        happens to carry, graph = adjacency-list degree."""
        if not query or not query.strip():
            return []
        query = query.strip()
        top_k = max(1, int(top_k))
        candidates = [n for n in self._nodes.values() if n.properties.get("status") != "DEPRECATED"]
        if project:
            candidates = [n for n in candidates if n.project == project]

        terms = [t.lower() for t in query.split() if t]
        lexical_raw: dict[str, float] = {}
        for n in candidates:
            label = n.label.lower()
            score = sum(1.0 for t in terms if t in label)
            if score > 0:
                lexical_raw[n.id] = score

        dense_raw: dict[str, float] = {}
        try:
            query_vector = embed_text(query, input_type="query")
        except Exception:  # noqa: BLE001 - degrade, don't raise
            query_vector = []
        if query_vector:
            for n in candidates:
                if n.embedding:
                    sim = cosine_similarity(query_vector, n.embedding)
                    if sim > 0:
                        dense_raw[n.id] = sim

        candidate_ids = set(lexical_raw) | set(dense_raw)
        graph_raw = {nid: float(len(self._adj.get(nid, []))) for nid in candidate_ids}

        lexical_scores = self._hybrid_normalized(lexical_raw)
        dense_scores = self._hybrid_normalized(dense_raw)
        graph_scores = self._hybrid_normalized(graph_raw)

        out: list[HybridMatch] = []
        for nid in candidate_ids:
            lex = lexical_scores.get(nid, 0.0)
            den = dense_scores.get(nid, 0.0)
            grf = graph_scores.get(nid, 0.0)
            score = (
                lex * self._HYBRID_LEXICAL_WEIGHT
                + den * self._HYBRID_DENSE_WEIGHT
                + grf * self._HYBRID_GRAPH_WEIGHT
            )
            out.append(HybridMatch(
                node=self._nodes[nid], score=score,
                lexical_score=lex, dense_score=den, graph_score=grf,
            ))
        out.sort(key=lambda m: m.score, reverse=True)
        return out[:top_k]

    def class_hierarchy(self, class_name: str) -> dict:
        """Mirrors Neo4jRepository.class_hierarchy: ancestors/descendants
        transitive over `extends` edges, interfaces/implementers direct
        only over `implements` edges (see that method's docstring for why)."""
        needle = class_name.lower()
        candidates = [
            (
                0 if n.label.lower() == needle else 1,
                0 if n.kind == NodeKind.CLASS.value else 1,
                n.label,
                nid,
            )
            for nid, n in self._nodes.items()
            if needle in n.label.lower()
        ]
        if not candidates:
            return {}
        candidates.sort()
        root_id = candidates[0][3]
        root = self._nodes[root_id]

        def _transitive_extends(start: str, outgoing: bool) -> list[Node]:
            seen: set[str] = {start}
            out: list[Node] = []
            frontier = [start]
            for _ in range(10):
                nxt: list[str] = []
                for cur in frontier:
                    for other, e in self._adj.get(cur, []):
                        if e.relation != "extends":
                            continue
                        matches = (e.source == cur) if outgoing else (e.target == cur)
                        if not matches or other in seen:
                            continue
                        seen.add(other)
                        nxt.append(other)
                        if other in self._nodes:
                            out.append(self._nodes[other])
                frontier = nxt
                if not frontier:
                    break
            return out

        def _direct_implements(start: str, outgoing: bool) -> list[Node]:
            out: list[Node] = []
            for other, e in self._adj.get(start, []):
                if e.relation != "implements":
                    continue
                matches = (e.source == start) if outgoing else (e.target == start)
                if matches and other in self._nodes:
                    out.append(self._nodes[other])
            return out

        return {
            "node": root,
            "ancestors": _transitive_extends(root_id, outgoing=True),
            "interfaces": _direct_implements(root_id, outgoing=True),
            "descendants": _transitive_extends(root_id, outgoing=False),
            "implementers": _direct_implements(root_id, outgoing=False),
        }

    def test_map(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Mirrors Neo4jRepository.test_map: reverse `TESTS`-edge lookup."""
        target_ids = self._resolve_symbol_ids(symbol)
        if not target_ids:
            return []
        out = [
            EdgeRecord(edge=e, source_label=self._label(e.source), target_label=self._label(e.target))
            for target_id in target_ids
            for _, e in self._adj.get(target_id, [])
            if e.relation == "TESTS" and e.target == target_id
        ]
        out.sort(key=lambda er: er.source_label)
        return out[: max(1, int(limit))]

    # -- Section 13 (Code Intelligence) shared plumbing -----------------------

    def code_symbol_nodes(self, project: str = "") -> list[Node]:
        """Mirrors Neo4jRepository.code_symbol_nodes."""
        out = [
            n for n in self._nodes.values()
            if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value) and n.source_file
        ]
        if project:
            out = [n for n in out if n.project == project]
        out.sort(key=lambda n: (n.source_file, n.line_start))
        return out

    def analysis_nodes(self, kind: str, project: str = "") -> list[Node]:
        """Mirrors Neo4jRepository.analysis_nodes."""
        out = [n for n in self._nodes.values() if n.kind == kind]
        if project:
            out = [n for n in out if n.project == project]
        return out

    def replace_analysis_nodes(
        self, kind: str, nodes: Sequence[dict], edges: Sequence[dict],
        project: str = "",
    ) -> int:
        """Mirrors Neo4jRepository.replace_analysis_nodes: drop every
        existing node of `kind` (+ edges touching it) scoped to
        `project`, then add the fresh set — same idempotent-replace
        contract, in plain Python dicts/lists instead of Cypher."""
        stale_ids = {
            nid for nid, n in self._nodes.items()
            if n.kind == kind and (not project or n.project == project)
        }
        for nid in stale_ids:
            del self._nodes[nid]
        self._edges = [
            e for e in self._edges
            if e.source not in stale_ids and e.target not in stale_ids
        ]
        node_named_keys = {
            "id", "label", "source_file", "source_location", "file_type",
            "kind", "signature", "line_start", "line_end", "docstring",
            "project",
        }
        for row in nodes:
            row = dict(row)
            if project:
                row["project"] = project
            node = Node(
                id=row["id"], label=row.get("label", row["id"]),
                source_file=row.get("source_file", ""),
                source_location=row.get("source_location", ""),
                file_type=row.get("file_type", ""), kind=row.get("kind", kind),
                signature=row.get("signature", ""),
                line_start=row.get("line_start", 0), line_end=row.get("line_end", 0),
                docstring=row.get("docstring", ""), project=row.get("project", ""),
                properties={k: v for k, v in row.items() if k not in node_named_keys},
            )
            self._nodes[node.id] = node
        edge_named_keys = {"source", "target", "relation", "confidence"}
        for row in edges:
            self._edges.append(Edge(
                source=row["source"], target=row["target"],
                relation=row.get("relation", ""),
                confidence=Confidence(row.get("confidence", "INFERRED")),
                properties={k: v for k, v in row.items() if k not in edge_named_keys},
            ))
        self._rebuild_adjacency()
        return len(nodes)

    def update_node_properties(self, updates: dict[str, dict], project: str = "") -> int:
        """Mirrors Neo4jRepository.update_node_properties: merges extra
        properties onto an existing node. `Node` is frozen, so "patch"
        means replace the dict entry with a new instance.

        The real Neo4j path (`SET n += row`) is flat — it doesn't
        distinguish "a named `Node` field" from "an arbitrary extra
        property"; `_row_to_node` is what re-splits a raw row back into
        named fields vs the `.properties` catch-all on READ. This fake
        must mirror that split explicitly (Python has no equivalent
        flat-then-reparse step): a key matching one of `Node`'s OWN
        dataclass fields (e.g. `community`, used by CI-06's community
        detection) is set via `dataclasses.replace` on that field
        directly; anything else (e.g. CI-06/08's `complexity_class`/
        `hot_path`) lands in `.properties`, same as before. Confirmed
        live while writing `cie.community_detect`'s tests — routing
        `community` into `.properties` instead of the named field meant
        `list_communities()`/`get_community()` (which read the NAMED
        field) saw nothing, even though the "patch" itself reported
        success.
        """
        node_field_names = {f.name for f in dataclasses.fields(Node)}
        count = 0
        for nid, props in updates.items():
            existing = self._nodes.get(nid)
            if existing is None:
                continue
            if project and existing.project != project:
                continue
            named_updates = {k: v for k, v in props.items() if k in node_field_names}
            extra_updates = {k: v for k, v in props.items() if k not in node_field_names}
            merged_properties = dict(existing.properties)
            merged_properties.update(extra_updates)
            self._nodes[nid] = dataclasses.replace(
                existing, properties=merged_properties, **named_updates,
            )
            count += 1
        self._rebuild_adjacency()
        return count

    def merge_delta(
        self, nodes: Sequence[dict], edges: Sequence[dict], project: str = "",
    ) -> int:
        """Mirrors Neo4jRepository.merge_delta: MERGE-on-id, never a
        wholesale replace — an existing node with the same id is
        overwritten in place; everyone else is untouched."""
        node_named_keys = {
            "id", "label", "source_file", "source_location", "file_type",
            "kind", "signature", "line_start", "line_end", "docstring",
            "project", "community", "embedding", "extracted_at",
            "extractor_version", "source_ref",
        }
        for row in nodes:
            row = dict(row)
            if project:
                row["project"] = project
            self._nodes[row["id"]] = Node(
                id=row["id"], label=row.get("label", row["id"]),
                source_file=row.get("source_file", ""),
                source_location=row.get("source_location", ""),
                file_type=row.get("file_type", ""),
                kind=row.get("kind", NodeKind.SYMBOL.value),
                signature=row.get("signature", ""),
                line_start=row.get("line_start", 0), line_end=row.get("line_end", 0),
                docstring=row.get("docstring", ""), project=row.get("project", ""),
                community=row.get("community"),
                embedding=tuple(row.get("embedding") or ()),
                extracted_at=row.get("extracted_at", ""),
                extractor_version=row.get("extractor_version", ""),
                source_ref=row.get("source_ref", ""),
                properties={k: v for k, v in row.items() if k not in node_named_keys},
            )
        edge_named_keys = {"source", "target", "relation", "confidence"}
        for row in edges:
            key = (row["source"], row["target"], row.get("relation", ""))
            self._edges = [
                e for e in self._edges
                if (e.source, e.target, e.relation) != key
            ]
            self._edges.append(Edge(
                source=row["source"], target=row["target"],
                relation=row.get("relation", ""),
                confidence=Confidence(row.get("confidence", "EXTRACTED")),
                properties={k: v for k, v in row.items() if k not in edge_named_keys},
            ))
        self._rebuild_adjacency()
        return len(nodes)

    def accumulate_actual_calls(
        self, pairs: Sequence[dict], project: str = "",
    ) -> int:
        """Mirrors Neo4jRepository.accumulate_actual_calls: one running
        ACTUAL_CALL edge per (source, target) pair, counters accumulated
        across calls rather than overwritten."""
        if not pairs:
            return 0
        now = datetime.now(timezone.utc).isoformat()
        by_key = {
            (e.source, e.target): e
            for e in self._edges
            if e.relation == "ACTUAL_CALL"
        }
        for p in pairs:
            key = (p["source"], p["target"])
            latency_ms = float(p.get("latency_ms", 0.0))
            error_inc = 1 if p.get("is_error") else 0
            existing = by_key.get(key)
            if existing is None:
                call_count = 1
                total_latency = latency_ms
                error_count = error_inc
                first_seen = now
            else:
                call_count = existing.properties.get("call_count", 0) + 1
                total_latency = existing.properties.get("total_latency_ms", 0.0) + latency_ms
                error_count = existing.properties.get("error_count", 0) + error_inc
                first_seen = existing.properties.get("first_seen", now)
            edge = Edge(
                source=key[0], target=key[1], relation="ACTUAL_CALL",
                properties={
                    "call_count": call_count,
                    "total_latency_ms": total_latency,
                    "avg_latency_ms": total_latency / call_count,
                    "error_count": error_count,
                    "error_rate": error_count / call_count,
                    "first_seen": first_seen,
                    "last_seen": now,
                },
            )
            by_key[key] = edge
        self._edges = [e for e in self._edges if e.relation != "ACTUAL_CALL"] + list(by_key.values())
        self._rebuild_adjacency()
        return len(pairs)

    def actual_callers(self, symbol: str, limit: int = 30) -> list[EdgeRecord]:
        """Mirrors Neo4jRepository.actual_callers: reverse ACTUAL_CALL lookup."""
        target_ids = self._resolve_symbol_ids(symbol)
        if not target_ids:
            return []
        out = [
            EdgeRecord(edge=e, source_label=self._label(e.source), target_label=self._label(e.target))
            for target_id in target_ids
            for _, e in self._adj.get(target_id, [])
            if e.relation == "ACTUAL_CALL" and e.target == target_id
        ]
        out.sort(key=lambda er: er.edge.properties.get("call_count", 0), reverse=True)
        return out[: max(1, int(limit))]

    def publish_telemetry_event(
        self, channel: str, payload: dict, project: str = "",
    ) -> int:
        """Mirrors Neo4jRepository.publish_telemetry_event: per-
        (project, channel) monotonic seq counter + an appended event."""
        key = (project, channel)
        seq = self._telemetry_seq.get(key, 0) + 1
        self._telemetry_seq[key] = seq
        self._telemetry_events.append({
            "project": project, "channel": channel, "seq": seq,
            "payload": dict(payload),
            "published_at": datetime.now(timezone.utc).isoformat(),
        })
        return seq

    def read_telemetry_events(
        self, channel: str, after_seq: int = 0, limit: int = 100, project: str = "",
    ) -> list[dict]:
        """Mirrors Neo4jRepository.read_telemetry_events: oldest first,
        seq > after_seq, scoped to (project, channel)."""
        matches = [
            e for e in self._telemetry_events
            if e["project"] == project and e["channel"] == channel and e["seq"] > after_seq
        ]
        matches.sort(key=lambda e: e["seq"])
        return [
            {"seq": e["seq"], "channel": e["channel"], "payload": dict(e["payload"]), "published_at": e["published_at"]}
            for e in matches[: max(1, int(limit))]
        ]

    def delete_nodes(self, ids: Sequence[str], project: str = "") -> int:
        """Mirrors Neo4jRepository.delete_nodes."""
        count = 0
        for nid in ids:
            existing = self._nodes.get(nid)
            if existing is None:
                continue
            if project and existing.project != project:
                continue
            del self._nodes[nid]
            count += 1
        if count:
            self._edges = [
                e for e in self._edges
                if e.source in self._nodes and e.target in self._nodes
            ]
            self._rebuild_adjacency()
        return count

    def delete_nodes_before(self, ids: Sequence[str], cutoff_iso: str, project: str = "") -> int:
        """SF4: mirrors Neo4jRepository.delete_nodes_before — re-checks
        each node's OWN CURRENT extracted_at against cutoff_iso at delete
        time (this in-memory store's dict lookup IS the single "read
        current state" point, same as Neo4j's WHERE clause evaluating
        inside the delete transaction), so a caller's earlier point-in-
        time `ids` snapshot can't delete a node whose extracted_at has
        since been refreshed past the cutoff."""
        count = 0
        for nid in ids:
            existing = self._nodes.get(nid)
            if existing is None:
                continue
            if project and existing.project != project:
                continue
            if not (existing.extracted_at and existing.extracted_at < cutoff_iso):
                continue
            del self._nodes[nid]
            count += 1
        if count:
            self._edges = [
                e for e in self._edges
                if e.source in self._nodes and e.target in self._nodes
            ]
            self._rebuild_adjacency()
        return count

    def record_metric_snapshot(self, snapshot: MetricSnapshot) -> None:
        """Mirrors Neo4jRepository.record_metric_snapshot: append-only."""
        self._metric_snapshots.append(snapshot)

    def metric_trend(
        self, metric_type: str = "", limit: int = 20, project: str = "",
    ) -> list[MetricSnapshot]:
        """Mirrors Neo4jRepository.metric_trend."""
        out = list(self._metric_snapshots)
        if project:
            out = [s for s in out if s.project == project]
        if metric_type:
            out = [s for s in out if s.metric_type == metric_type]
        out.sort(key=lambda s: s.measured_at, reverse=True)
        return out[: max(1, int(limit))]

    def embedding_clone_pairs(
        self, threshold: float = 0.92, k: int = 5, project: str = "",
    ) -> list[tuple[str, str, float]]:
        """Mirrors Neo4jRepository.embedding_clone_pairs: brute-force
        pairwise cosine similarity (fine at test-fixture scale; the real
        implementation uses the vector index for the same reason
        `hybrid_search`'s dense leg does)."""
        candidates = [n for n in self.code_symbol_nodes(project=project) if n.embedding]
        out: list[tuple[str, str, float]] = []
        for i in range(len(candidates)):
            for j in range(i + 1, len(candidates)):
                a, b = candidates[i], candidates[j]
                score = cosine_similarity(a.embedding, b.embedding)
                if score >= threshold:
                    out.append((a.id, b.id, score))
        return out

    def health_details(self) -> dict:
        stats = self.stats()
        return {
            "store": "reachable",
            "nodes": stats.nodes,
            "edges": stats.edges,
            "communities": stats.communities,
            "project": "",
        }

    def semantic_search(
        self, query: str, top_k: int = 10, project: str = ""
    ) -> list[SemanticMatch]:
        """Mirrors Neo4jRepository.semantic_search using the same
        `cosine_similarity` helper against whatever `node.embedding` each
        in-memory node happens to carry — `()` for most test fixtures,
        which just means those nodes never surface here (no crash)."""
        if not query:
            return []
        try:
            query_vector = embed_text(query)
        except Exception:  # noqa: BLE001 - degrade to "no matches"
            return []
        if not query_vector:
            return []
        candidates = [n for n in self._nodes.values() if n.properties.get("status") != "DEPRECATED"]
        if project:
            candidates = [n for n in candidates if n.project == project]
        scored = [
            SemanticMatch(node=n, score=cosine_similarity(query_vector, n.embedding))
            for n in candidates
            if n.embedding
        ]
        scored.sort(key=lambda m: m.score, reverse=True)
        return scored[: max(1, int(top_k))]

    # -- QA coverage (mirrors Neo4jRepository's exact-match /
    # never-measured-vs-zero-percent semantics; see that class for why) --

    def record_coverage(
        self, file_path: str, subtree: str, coverage_pct: float,
        covered_lines: int, uncovered_lines, measured_at: str,
    ) -> Optional[FileCoverage]:
        uncovered = sorted({int(n) for n in uncovered_lines})
        match = next(
            (n for n in self._nodes.values()
             if n.source_file == file_path and n.kind == NodeKind.FILE.value),
            None,
        )
        if match is None:
            return None
        self._file_coverage[file_path] = {
            "coverage_pct": float(coverage_pct), "covered_lines": int(covered_lines),
            "uncovered_line_count": len(uncovered), "uncovered_lines": uncovered,
            "measured_at": measured_at, "subtree": subtree,
        }
        uncovered_set = set(uncovered)
        functions: list[FunctionCoverage] = []
        for n in self._nodes.values():
            if n.source_file != file_path or n.kind not in (
                NodeKind.FUNC.value, NodeKind.METHOD.value
            ):
                continue
            line_start, line_end = n.line_start or 0, n.line_end or 0
            if line_end < line_start:
                pct = 100.0
            else:
                total = line_end - line_start + 1
                miss = sum(1 for ln in range(line_start, line_end + 1) if ln in uncovered_set)
                pct = round((1 - miss / total) * 100, 1)
            self._func_coverage[n.id] = pct
            functions.append(FunctionCoverage(
                label=n.label, line_start=line_start, line_end=line_end, coverage_pct=pct,
            ))
        functions.sort(key=lambda f: f.line_start)
        return FileCoverage(
            file_path=file_path, coverage_pct=float(coverage_pct),
            covered_lines=int(covered_lines), uncovered_line_count=len(uncovered),
            uncovered_lines=tuple(uncovered), measured_at=measured_at, subtree=subtree,
            functions=tuple(functions),
        )

    def get_coverage(self, file_path: str, subtree: str = "") -> Optional[FileCoverage]:
        data = self._file_coverage.get(file_path)
        if data is None:
            return None
        if subtree and data["subtree"] != subtree:
            return None
        functions = [
            FunctionCoverage(
                label=n.label, line_start=n.line_start or 0, line_end=n.line_end or 0,
                coverage_pct=self._func_coverage[n.id],
            )
            for n in self._nodes.values()
            if n.source_file == file_path
            and n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value)
            and n.id in self._func_coverage
        ]
        functions.sort(key=lambda f: f.line_start)
        return FileCoverage(
            file_path=file_path, coverage_pct=data["coverage_pct"],
            covered_lines=data["covered_lines"],
            uncovered_line_count=data["uncovered_line_count"],
            uncovered_lines=tuple(data["uncovered_lines"]),
            measured_at=data["measured_at"], subtree=data["subtree"],
            functions=tuple(functions),
        )

    def coverage_report(
        self, subtree: str = "", below_pct: Optional[float] = None,
        file_glob: str = "", include_unmeasured: bool = True,
    ) -> list[FileCoverageSummary]:
        out: list[FileCoverageSummary] = []
        for n in self._nodes.values():
            if n.kind != NodeKind.FILE.value:
                continue
            fp = n.source_file
            if file_glob and not fnmatch.fnmatch(fp, file_glob):
                continue
            data = self._file_coverage.get(fp)
            if subtree and (data is None or data["subtree"] != subtree):
                continue
            if data is None:
                if not include_unmeasured:
                    continue
                out.append(FileCoverageSummary(
                    file_path=fp, measured=False, coverage_pct=None,
                    uncovered_line_count=0, measured_at="", subtree="",
                ))
            else:
                pct = data["coverage_pct"]
                if below_pct is not None and pct >= below_pct:
                    continue
                out.append(FileCoverageSummary(
                    file_path=fp, measured=True, coverage_pct=pct,
                    uncovered_line_count=data["uncovered_line_count"],
                    measured_at=data["measured_at"], subtree=data["subtree"],
                ))
        out.sort(key=lambda s: s.coverage_pct if s.coverage_pct is not None else -1.0)
        return out

    def record_coverage_snapshot(self, snapshot: CoverageSnapshot) -> None:
        self._snapshots.append(snapshot)

    def coverage_trend(self, subtree: str = "", limit: int = 20) -> list[CoverageSnapshot]:
        matches = [s for s in self._snapshots if not subtree or s.subtree == subtree]
        matches.sort(key=lambda s: s.measured_at, reverse=True)
        return matches[: max(1, int(limit))]


def _dict_to_node(d: dict) -> Node:
    return Node(
        id=d["id"],
        label=d.get("label", d["id"]),
        source_file=d.get("source_file", ""),
        source_location=d.get("source_location", ""),
        file_type=d.get("file_type", ""),
        community=d.get("community"),
        kind=d.get("kind", NodeKind.SYMBOL.value),
        signature=d.get("signature", ""),
        line_start=d.get("line_start", 0),
        line_end=d.get("line_end", 0),
        docstring=d.get("docstring", ""),
        project=d.get("project", ""),
        embedding=tuple(d.get("embedding") or ()),
    )


def _dict_to_edge(d: dict) -> Edge:
    try:
        conf = Confidence(d.get("confidence", "EXTRACTED"))
    except ValueError:
        conf = Confidence.EXTRACTED
    return Edge(
        source=d["source"],
        target=d["target"],
        relation=d.get("relation", ""),
        confidence=conf,
    )


def _parse_params(signature: str) -> tuple[str, ...]:
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
        name = raw.split(":")[0].strip()
        name = name.split("=")[0].strip()
        if name:
            params.append(name)
    return tuple(params)


def _parse_return_type(signature: str) -> str:
    if not signature:
        return ""
    idx = signature.rfind("->")
    if idx == -1:
        return ""
    return signature[idx + 2:].strip()
