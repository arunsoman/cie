"""CI-01/02/03/04/05: Clone Detector.

Three independent, complementary signals, fused into `CloneCluster`
graph nodes:

- **Token** (CI-01): exact leaf-token Jaccard similarity over each
  function's re-parsed body — catches copy-paste clones (same names,
  same structure). Pure, computed via `cie.source_analysis`.
- **AST** (CI-02): structural-shape Jaccard similarity (node TYPES only,
  never text) — catches the same control-flow shape even with every
  identifier renamed. Pure, same re-parse pass as the token signal.
- **Embedding** (CI-03): cosine similarity between already-computed node
  embeddings via Neo4j's vector index — catches semantic equivalents
  with completely different names AND structure. Needs a live
  `Neo4jRepository` (embeddings only exist after a real load with
  `NVIDIA_API_KEY` set), so this leg is NOT pure like the other two.

Each signal alone misses clones the others catch (this is section 13.1's
own framing, not new reasoning here) — `resolve_clone_clusters` fuses all
three via union-find: two symbols end up in the same `CloneCluster` if
EITHER signal linked them, directly or transitively through a chain of
pairwise links.

This is an explicit, on-demand, whole-project analysis pass (spec
section 13.5's "scheduled/batch" framing) — NOT wired into
`reindex_file`'s per-file hot path, and it does not replace or touch the
structural code graph; it only adds/replaces its own `CloneCluster`
nodes via `Neo4jRepository.replace_analysis_nodes`.
"""
from __future__ import annotations

import hashlib
import math
from collections import defaultdict

from cie.models import Confidence, Node, NodeKind
from cie.repository import Repository
from cie.source_analysis import SourceIndex, leaf_tokens, structural_shape

_TOKEN_JACCARD_THRESHOLD = 0.8
_AST_JACCARD_THRESHOLD = 0.8
_EMBEDDING_COSINE_THRESHOLD = 0.92

# Coarse size-bucket tolerance for candidate pruning (see `_bucket_key`) —
# without this, comparing every function against every other function is
# O(n^2) across a whole project; bucketing by (rounded) token/node count
# means only reasonably-similar-sized functions are ever compared.
_BUCKET_TOLERANCE = 0.2

# Hard cap on pairwise comparisons within one size bucket — a project with
# thousands of near-identical-length trivial functions (e.g. generated
# boilerplate) could otherwise still blow up one bucket's own O(n^2). Past
# this many candidates in a bucket, only the first `_MAX_BUCKET_SIZE` are
# compared — a documented recall tradeoff, not silently wrong, just
# incomplete at extreme scale.
_MAX_BUCKET_SIZE = 300


def _jaccard(a: list[str], b: list[str]) -> float:
    sa, sb = set(a), set(b)
    if not sa or not sb:
        return 0.0
    union = len(sa | sb)
    return len(sa & sb) / union if union else 0.0


def _bucket_key(count: int) -> int:
    """Log-scale bucket: two counts within roughly a
    `1 + _BUCKET_TOLERANCE` multiplicative ratio of each other usually
    land in the same (or an adjacent) bucket, while counts far apart
    (10 vs 1000) always land in different ones. A LINEAR bucket
    (`count // (count * tolerance)`) degenerates to a near-constant
    value for any count once tolerance is fixed — confirmed while
    writing this, not a hypothetical: `10 // 2` and `1000 // 200` are
    both `5`, which would put every function in the project in
    virtually the same bucket regardless of size, defeating the whole
    point of pruning. Log-scale avoids that."""
    if count <= 0:
        return 0
    return int(math.log(count, 1 + _BUCKET_TOLERANCE))


class _UnionFind:
    """Minimal union-find over string ids — fuses CI-01/02/03's pairwise
    signals into connected-component clusters (CI-04)."""

    def __init__(self) -> None:
        self._parent: dict[str, str] = {}

    def find(self, x: str) -> str:
        self._parent.setdefault(x, x)
        root = x
        while self._parent[root] != root:
            root = self._parent[root]
        while self._parent[x] != root:
            self._parent[x], x = root, self._parent[x]
        return root

    def union(self, a: str, b: str) -> None:
        ra, rb = self.find(a), self.find(b)
        if ra != rb:
            self._parent[ra] = rb


def find_token_ast_clones(
    nodes: list[Node], source_index: SourceIndex,
) -> list[tuple[str, str, float, str]]:
    """CI-01 + CI-02: re-parse each of `nodes`' source spans and return
    every pair scoring at or above threshold on the token signal, the AST
    signal, or both — as `(id_a, id_b, similarity, method)` where
    `method` is `"token"` or `"ast"`. A pair strong on both signals
    appears twice (once per method); `resolve_clone_clusters` fuses
    duplicates via union-find, so this is harmless, not wasted work — the
    caller (a tool/report) can still see BOTH signals fired.

    Nodes whose source can't be re-located (file moved/deleted since the
    graph was last loaded, or a line-number mismatch from a stale graph)
    are silently skipped — same "stale graph, not a bug" contract as
    `SourceIndex.locate`.
    """
    token_shapes: dict[str, list[str]] = {}
    ast_shapes: dict[str, list[str]] = {}
    for n in nodes:
        if not n.source_file:
            continue
        ts_node = source_index.locate(n.source_file, n.line_start, n.kind)
        if ts_node is None:
            continue
        token_shapes[n.id] = leaf_tokens(ts_node)
        ast_shapes[n.id] = structural_shape(ts_node)

    pairs: list[tuple[str, str, float, str]] = []

    def _compare(shapes: dict[str, list[str]], method: str, threshold: float) -> None:
        buckets: dict[int, list[str]] = defaultdict(list)
        for node_id, shape in shapes.items():
            buckets[_bucket_key(len(shape))].append(node_id)
        for bucket_ids in buckets.values():
            capped = bucket_ids[:_MAX_BUCKET_SIZE]
            for i in range(len(capped)):
                for j in range(i + 1, len(capped)):
                    a, b = capped[i], capped[j]
                    score = _jaccard(shapes[a], shapes[b])
                    if score >= threshold:
                        pairs.append((a, b, round(score, 4), method))

    _compare(token_shapes, "token", _TOKEN_JACCARD_THRESHOLD)
    _compare(ast_shapes, "ast", _AST_JACCARD_THRESHOLD)
    return pairs


def resolve_clone_clusters(
    nodes_by_id: dict[str, Node],
    pairs: list[tuple[str, str, float, str]],
) -> tuple[list[dict], list[dict]]:
    """CI-04: fuse pairwise signals (from `find_token_ast_clones` and/or
    `Neo4jRepository.embedding_clone_pairs`) into `CloneCluster` node
    dicts + `MEMBER_OF`/`SIMILAR_TO` edge dicts, ready for
    `Neo4jRepository.replace_analysis_nodes("CloneCluster", ...)`.

    A cluster's id is a deterministic hash of its SORTED member ids, so
    re-running detection on an unchanged codebase reproduces the exact
    same cluster ids (idempotent, matching every other write path in this
    package). The member with the highest total similarity to its
    clustermates (ties broken by shortest id, for determinism) is marked
    the `consolidation_target`.
    """
    uf = _UnionFind()
    pair_index: dict[tuple[str, str], list[tuple[float, str]]] = defaultdict(list)
    for a, b, score, method in pairs:
        if a not in nodes_by_id or b not in nodes_by_id:
            continue
        uf.union(a, b)
        pair_index[(a, b)].append((score, method))

    groups: dict[str, set[str]] = defaultdict(set)
    for a, b in pair_index:
        groups[uf.find(a)].add(a)
        groups[uf.find(a)].add(b)

    cluster_nodes: list[dict] = []
    edges: list[dict] = []
    for members in groups.values():
        if len(members) < 2:
            continue
        member_ids = sorted(members)
        cluster_id = "clonecluster::" + hashlib.sha1(
            "|".join(member_ids).encode("utf-8")
        ).hexdigest()[:16]

        # per-member: best similarity to any other member, and every
        # detection method that fired for at least one of its pairs.
        best_score: dict[str, float] = defaultdict(float)
        methods: dict[str, set[str]] = defaultdict(set)
        for (a, b), scored in pair_index.items():
            if a not in members or b not in members:
                continue
            for score, method in scored:
                best_score[a] = max(best_score[a], score)
                best_score[b] = max(best_score[b], score)
                methods[a].add(method)
                methods[b].add(method)

        consolidation_target = sorted(
            member_ids, key=lambda m: (-best_score[m], len(m), m),
        )[0]

        cluster_nodes.append({
            "id": cluster_id,
            "label": f"clone cluster ({len(member_ids)} members)",
            "source_file": "",
            "source_location": "",
            "file_type": "analysis",
            "kind": NodeKind.CLONE_CLUSTER.value,
            "signature": "",
            "line_start": 0,
            "line_end": 0,
            "docstring": "",
            "member_count": len(member_ids),
            "consolidation_target": consolidation_target,
        })
        for member_id in member_ids:
            edges.append({
                "source": member_id,
                "target": cluster_id,
                "relation": "MEMBER_OF",
                "confidence": Confidence.INFERRED.value,
                "similarity": round(best_score[member_id], 4),
                "detected_by": ",".join(sorted(methods[member_id])),
            })

    return cluster_nodes, edges


def analyze(
    repo: Repository, project: str = "",
    embedding_threshold: float = _EMBEDDING_COSINE_THRESHOLD,
) -> dict:
    """Run the full clone-detection pass against `repo`'s current graph
    and write the result via `replace_analysis_nodes` — the single entry
    point `ToolService.clone_detect_run`/`cie analyze-clones` calls.

    Returns a summary dict: cluster/member counts and CI-05's
    `clone_coverage_pct` (member symbols / total FUNC+METHOD symbols).
    Never raises for "nothing found" (an empty project, or a project
    with no clones) — returns a summary with zero clusters instead.
    """
    candidates = repo.code_symbol_nodes(project=project)
    nodes_by_id = {n.id: n for n in candidates}
    if not candidates:
        repo.replace_analysis_nodes(NodeKind.CLONE_CLUSTER.value, [], [], project=project)
        return {
            "clusters": 0, "clustered_symbols": 0, "total_symbols": 0,
            "clone_coverage_pct": 0.0,
        }

    source_index = SourceIndex()
    pairs = find_token_ast_clones(candidates, source_index)
    pairs.extend(
        (a, b, score, "embedding")
        for a, b, score in repo.embedding_clone_pairs(
            threshold=embedding_threshold, project=project,
        )
    )
    cluster_nodes, edges = resolve_clone_clusters(nodes_by_id, pairs)
    repo.replace_analysis_nodes(
        NodeKind.CLONE_CLUSTER.value, cluster_nodes, edges, project=project,
    )
    clustered_ids: set[str] = set()
    for edge in edges:
        clustered_ids.add(edge["source"])
    total = len(candidates)
    coverage = round((len(clustered_ids) / total) * 100, 2) if total else 0.0
    return {
        "clusters": len(cluster_nodes),
        "clustered_symbols": len(clustered_ids),
        "total_symbols": total,
        "clone_coverage_pct": coverage,
    }
