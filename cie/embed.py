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
import json
import logging
import math
import os
import urllib.request
from typing import List, Optional, Sequence

try:  # be-v2's real embedding client, when cie runs embedded in be-v2
    from core.llm.embed_text import embed_text as _real_embed_text  # type: ignore
    from core.llm.embed_text import embed_texts as _real_embed_texts  # type: ignore
except Exception:  # standalone shim — cie installs without be-v2 on the path
    _real_embed_text = None
    _real_embed_texts = None


# ---------------------------------------------------------------------------
# First-party OpenAI-compatible fallback (stdlib-only, env-gated).
#
# Dispatch order (strongest wins):
#   1. `core.llm` (be-v2's client) — when cie runs inside the host.
#   2. `register_embed_functions` override — a host explicitly handing
#      cie its own impl beats everything config-derived.
#   3. THIS env-gated first-party client — stdlib urllib POST to any
#      OpenAI-compatible `/embeddings` endpoint. The benchmark harness
#      (scripts/benchmark_semantic.py) documents the resolved config.
#   4. Raise — every call site already wraps its embed call in
#      `try/except Exception: degrade to []` (see e.g.
#      `Neo4jRepository.semantic_search`), so raising feeds that
#      documented degrade path.
#
# Deliberate no-accidental-network rule: the fallback is active ONLY
# when `CIE_EMBED_DSN` is explicitly set AND a key is available
# (`CIE_EMBED_API_KEY`, else `NVIDIA_API_KEY`). Setting a bare API key
# with no DSN never turns network calls on — the default DSN (NIM) is
# only substituted once the user has pointed cie at a endpoint by
# setting the DSN at all. This keeps `pytest -q` and any CI run free of
# surprise HTTP even on a machine whose shell carries a provider key.

#: NVIDIA NIM's OpenAI-compatible embeddings base URL — the same API
#: shape core.llm targets when cie runs in the host environment.
_DEFAULT_OPENAI_COMPATIBLE_DSN = "https://integrate.api.nvidia.com/v1"

#: Default model. NVIDIA's retrieval-optimized embedding (nv-embedqa
#: family) honors OpenAI's input-format while additionally requiring the
#: `input_type` field ("query" vs "passage") — which the existing call
#: sites already pass through (see `embedded_repository.hybrid_search`).
_DEFAULT_OPENAI_COMPATIBLE_MODEL = "nvidia/nv-embedqa-e5-v5"


def _env_fallback_config() -> Optional[tuple[str, str, str]]:
    """Resolve the env-gated fallback config, or None when not active.

    Returns ``(dsn, api_key, model)``. Read lazily at call time (not
    import time) so tests and benchmark scripts can set/clear the env
    freely. See the block comment above for the deliberate gating rule:
    `CIE_EMBED_DSN` must be set; the key may come from `CIE_EMBED_API_KEY`
    or fall back to `NVIDIA_API_KEY`.
    """
    dsn = (os.environ.get("CIE_EMBED_DSN") or "").strip().rstrip("/")
    if not dsn:
        return None
    api_key = (
        os.environ.get("CIE_EMBED_API_KEY") or os.environ.get("NVIDIA_API_KEY") or ""
    ).strip()
    if not api_key:
        return None
    model = (
        os.environ.get("CIE_EMBED_MODEL") or _DEFAULT_OPENAI_COMPATIBLE_MODEL
    ).strip()
    return dsn, api_key, model


def _post_embeddings(
    dsn: str, api_key: str, model: str,
    texts: Sequence[str], input_type: str,
) -> list[list[float]]:
    """One POST to `<dsn>/embeddings` via stdlib urllib — no client
    dependency. Returns vectors in the same order as `texts` (the API
    indexes each item; we re-sort in case the server reorders).

    Raises on any HTTP/JSON/failure-path problem so the caller's
    existing degrade-or-retry handling stays in charge.
    """
    req = urllib.request.Request(
        f"{dsn}/embeddings",
        method="POST",
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
        data=json.dumps({
            "input": list(texts),
            "model": model,
            "input_type": input_type,  # nv-embedqa family requires it
            "encoding_format": "float",
        }).encode("utf-8"),
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        body = json.loads(resp.read().decode("utf-8"))
    # OpenAI-compatible shape: {"data": [{"index": i, "embedding": [...]}, ...]}
    entries = sorted(body.get("data", []), key=lambda e: e.get("index", 0))
    vectors = [list(map(float, e["embedding"])) for e in entries]
    if len(vectors) != len(texts):
        raise RuntimeError(
            f"embeddings endpoint returned {len(vectors)} vectors for "
            f"{len(texts)} inputs"
        )
    return vectors


def _openai_compatible_embed_texts(
    texts: Sequence[str], input_type: str = "passage",
) -> list[list[float]]:
    """Env-fallback batch entry — dispatch tier 3. Raises RuntimeError
    when the env gate isn't satisfied (caller falls through to the
    raising shim, or handles per its own degrade contract)."""
    cfg = _env_fallback_config()
    if cfg is None:
        raise RuntimeError(
            "no first-party embeddings fallback configured — set CIE_EMBED_DSN "
            "(+ CIE_EMBED_API_KEY or NVIDIA_API_KEY) to enable the "
            "OpenAI-compatible client, or register an override via "
            "cie.embed.register_embed_functions"
        )
    return _post_embeddings(*cfg, texts=texts, input_type=input_type)


def supports_embeddings() -> bool:
    """True when some embeddings implementation is available for real
    HTTP-free-vs-HTTP decisions at load time — `core.llm` present, an
    override registered, or the env-gated first-party client configured.
    Mirrors the gate `Neo4jRepository._maybe_compute_embeddings` uses.
    """
    if _real_embed_texts is not None:
        return True
    return _env_fallback_config() is not None


def embed_text(text: str, model_name: Optional[str] = None, input_type: str = "passage") -> list[float]:
    """Dispatches to be-v2's real `core.llm.embed_text.embed_text` when
    available, else to whatever `register_embed_functions` registered,
    else raises. A stable function object (unlike rebinding this name
    directly) so `from cie.embed import embed_text` elsewhere in this
    package — `cie.neo4j_repository` imports it this way rather than
    duplicating this shim — keeps working even if an override is
    registered AFTER that import already ran.

    No literal-duplicate stand-in makes sense here (embed_text is a real
    HTTP call to an embeddings provider, not a small pure value like
    hierarchy.py's rel_type_union) — every call site already wraps its
    embed_text/embed_texts call in `try/except Exception: degrade to []`
    (see e.g. `Neo4jRepository.semantic_search`), so raising here just
    feeds that existing degrade-gracefully path instead of failing
    `import cie.embed` itself.
    """
    if _real_embed_text is not None:
        return _real_embed_text(text, model_name=model_name, input_type=input_type)
    try:
        return _openai_compatible_embed_texts([text], input_type=input_type)[0]
    except RuntimeError:
        raise RuntimeError(
            "no embed_text implementation available — core.llm is not on "
            "the path, no override was registered via "
            "cie.embed.register_embed_functions, and no first-party "
            "OpenAI-compatible endpoint is configured (set CIE_EMBED_DSN "
            "+ CIE_EMBED_API_KEY / NVIDIA_API_KEY"
        ) from None


def embed_texts(
    texts: Sequence[str], model_name: Optional[str] = None, input_type: str = "passage",
) -> list[list[float]]:
    """Batch form — see `embed_text`'s docstring for the dispatch/shim
    contract, identical here."""
    if _real_embed_texts is not None:
        return _real_embed_texts(texts, model_name=model_name, input_type=input_type)
    try:
        return _openai_compatible_embed_texts(texts, input_type=input_type)
    except RuntimeError:
        raise RuntimeError(
            "no embed_texts implementation available — core.llm is not on "
            "the path, no override was registered via "
            "cie.embed.register_embed_functions, and no first-party "
            "OpenAI-compatible endpoint is configured (set CIE_EMBED_DSN "
            "+ CIE_EMBED_API_KEY / NVIDIA_API_KEY"
        ) from None


def register_embed_functions(single, batch) -> None:
    """Override the embedding functions `embed_text`/`embed_texts` above
    dispatch to — for a host project with no `core.llm` on its path that
    still wants real embeddings. `single(text, model_name=None,
    input_type="passage") -> list[float]`, `batch(texts, model_name=None,
    input_type="passage") -> list[list[float]]`, matching
    `core.llm.embed_text`'s own signatures."""
    global _real_embed_text, _real_embed_texts
    _real_embed_text = single
    _real_embed_texts = batch

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
    """Cosine similarity between two vectors; 0.0 for empty/zero vectors
    OR mismatched dimensionality.

    Pure Python (no numpy). Returns 0.0 rather than raising when either
    vector is empty, has zero magnitude, or the two vectors aren't the
    same length (SF3: a node carrying an embedding from a different
    model/version than another — post-upgrade, or backfilled by a
    different path — used to get silently truncated to the shorter
    vector's prefix and compared anyway, producing a plausible-looking
    but meaningless score instead of the same "no match" signal already
    used for every other degenerate input) — an embedding-less OR
    incompatible-dimension node should score as "no match", not crash
    the ranking or return a bogus number.
    """
    if not a or not b or len(a) != len(b):
        return 0.0
    n = len(a)
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
