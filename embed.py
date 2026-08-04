"""Semantic embedding helpers for the code graph.

Wraps `core.llm.embed_text.embed_text` (NVIDIA NIM's OpenAI-compatible
embeddings endpoint) with the two things cie's loader needs on top of
a single blocking call: a consistent per-node text-to-embed builder, and a
bounded-concurrency batch runner so loading a graph with thousands of nodes
doesn't serialize thousands of network round-trips.

Cosine similarity is implemented in pure Python (no numpy dependency) since
this repo doesn't otherwise need it.
"""

from __future__ import annotations

import concurrent.futures
import logging
import math
from typing import List, Sequence

from core.llm.embed_text import embed_text

logger = logging.getLogger("cie.embed")


def node_embedding_text(node_dict: dict) -> str:
    """Build the text to embed for one node dict.

    Uses whichever of `label`/`kind`/`source_file`/`docstring`/`signature`
    are present and non-empty. `label`/`kind`/`source_file` form a header
    line even when some of them are blank (so a bare label still embeds to
    something useful); `docstring`/`signature` are appended as extra lines
    only when non-empty.
    """
    label = node_dict.get("label") or node_dict.get("id", "")
    kind = node_dict.get("kind", "")
    source_file = node_dict.get("source_file", "")
    docstring = node_dict.get("docstring", "")
    signature = node_dict.get("signature", "")

    lines = [f"{label} ({kind}) in {source_file}"]
    if docstring:
        lines.append(docstring)
    if signature:
        lines.append(signature)
    return "\n".join(lines).strip()


def compute_embeddings(nodes: List[dict], max_workers: int = 12) -> None:
    """Mutate each node dict in place, adding an `embedding` key.

    Uses a bounded `ThreadPoolExecutor` since `embed_text` is a blocking
    network call — sequential would be far too slow for graphs with
    thousands of nodes, but firing one request per node unbounded would
    hammer the embeddings API. Per-node failures are caught and logged, not
    raised: a node that fails to embed keeps `embedding: []` rather than
    aborting the whole batch, so a code graph still loads even if the
    embeddings API is briefly flaky.
    """
    if not nodes:
        return

    def _embed_one(node: dict) -> None:
        try:
            node["embedding"] = embed_text(node_embedding_text(node))
        except Exception:  # noqa: BLE001 - keep the batch alive
            logger.warning(
                "embedding failed for node %r; leaving embedding empty",
                node.get("id"),
                exc_info=True,
            )
            node["embedding"] = []

    with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as pool:
        # list() forces every future to complete (and any exception inside
        # _embed_one is already swallowed there) before returning.
        list(pool.map(_embed_one, nodes))


def cosine_similarity(a: Sequence[float], b: Sequence[float]) -> float:
    """Cosine similarity between two vectors; 0.0 for empty/zero vectors.

    Pure Python (no numpy). Returns 0.0 rather than raising when either
    vector is empty or has zero magnitude — an embedding-less node should
    score as "no match", not crash the ranking.
    """
    if not a or not b:
        return 0.0
    n = min(len(a), len(b))
    dot = 0.0
    norm_a = 0.0
    norm_b = 0.0
    for i in range(n):
        x = a[i]
        y = b[i]
        dot += x * y
        norm_a += x * x
        norm_b += y * y
    if norm_a <= 0.0 or norm_b <= 0.0:
        return 0.0
    return dot / (math.sqrt(norm_a) * math.sqrt(norm_b))
