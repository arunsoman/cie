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

from core.llm.embed_text import embed_text, embed_texts

logger = logging.getLogger("cie.embed")

#: Texts per `embeddings.create` call when batching (compute_embeddings
#: below). NVIDIA NIM's OpenAI-compatible embeddings endpoint accepts a
#: list `input`, same as OpenAI's own — one HTTP round trip for a batch of
#: this many nodes instead of one round trip per node. Kept well under
#: typical embeddings-endpoint batch caps (commonly 32-96) since node text
#: length varies (docstring/signature can be long) and this trades off
#: against per-request payload size, not just item count.
_EMBED_BATCH_SIZE = 16


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

    Batches `_EMBED_BATCH_SIZE` nodes per `embeddings.create` call instead
    of one call per node — 12 concurrent single-text requests used to mean
    12x HTTP/connection overhead for what the embeddings endpoint can
    already accept as one list `input`. Batches run concurrently through a
    bounded `ThreadPoolExecutor` (same reasoning as before: sequential is
    too slow for a large graph, unbounded hammers the API) — just one
    thread per BATCH now, not per node. A batch call failing wholesale
    (one bad text, a transient error affecting the whole request) falls
    back to embedding that batch's nodes one at a time rather than losing
    every node in it; a node that fails even that keeps `embedding: []`
    exactly as before, so the graph still loads on a flaky embeddings API.
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

    def _embed_batch(batch: List[dict]) -> None:
        texts = [node_embedding_text(n) for n in batch]
        try:
            vectors = embed_texts(texts)
            if len(vectors) != len(batch):
                raise RuntimeError(
                    f"embeddings endpoint returned {len(vectors)} vectors "
                    f"for {len(batch)} inputs"
                )
            for node, vector in zip(batch, vectors):
                node["embedding"] = vector
        except Exception:  # noqa: BLE001 - fall back to per-node, not a lost batch
            logger.warning(
                "batch embedding failed for %d node(s); falling back to "
                "per-node embedding for this batch",
                len(batch), exc_info=True,
            )
            for node in batch:
                _embed_one(node)

    batches = [
        nodes[i:i + _EMBED_BATCH_SIZE] for i in range(0, len(nodes), _EMBED_BATCH_SIZE)
    ]
    with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as pool:
        # list() forces every future to complete (and any exception inside
        # _embed_batch is already swallowed there) before returning.
        list(pool.map(_embed_batch, batches))


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
