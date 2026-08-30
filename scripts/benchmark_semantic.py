#!/usr/bin/env python3
"""R10's first-party semantic-retrieval benchmark (reproducible from this
repo alone).

Measures cie's embedding-backed retrieval — `hybrid_search` (lexical +
dense + graph) and `semantic_search` (dense) — against hand-labeled
relevance sets on the same corpora as R9's benchmarks (psf/requests and
urllib3 at their pinned commits), plus the layers R10 names:

  precision/recall@k and MRR (file level, k = 8 unique files)
  context token estimate per query (graphrag._assemble_context — the
      pure, R10-decoupled context-assembly layer; the LLM answer step of
      `qa` stays host-gated and is NOT exercised here)
  index time: plain vs with-embeddings, same corpus, same machine
  naive-side calibration: a `grep -ril` hit-list payload for the same
      question, in chars, so "context per query" has an external anchor
      (char-based, tokenizer-free — same convention as R9)

Honesty rules kept from R9: relevance labels are hand-derived by reading
the repos (grep evidence recorded inline per label); competitor
embedding-search stacks (claude-context, grepai) are stated as
vendor-documented in the published doc and are NOT measured by this
script (running them requires their own services/indices — the doc says
which); the misses are published with the wins.

First-party fallback config (see cie/embed.py): requires
    CIE_EMBED_DSN=https://integrate.api.nvidia.com/v1
    NVIDIA_API_KEY=...            (or CIE_EMBED_API_KEY=...)
    CIE_EMBED_MODEL=nvidia/nemotron-3-embed-1b   (the model the
                    2026-08-31 measurement used; override freely)
The script refuses to run without these — silently falling back to
lexical-only would measure nothing.

Usage:
  python scripts/benchmark_semantic.py --project /path/to/repo \
      --corpus requests [--top-k 8] [--out results.json]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT))


def _require_env() -> dict[str, str]:
    dsn = os.environ.get("CIE_EMBED_DSN", "").strip()
    key = (os.environ.get("CIE_EMBED_API_KEY")
           or os.environ.get("NVIDIA_API_KEY") or "").strip()
    model = os.environ.get("CIE_EMBED_MODEL", "nvidia/nemotron-3-embed-1b")
    if not dsn or not key:
        sys.exit(
            "error: set CIE_EMBED_DSN and CIE_EMBED_API_KEY/NVIDIA_API_KEY "
            "(see cie/embed.py's gating rule — a bare key alone never "
            "enables network calls)"
        )
    return {"dsn": dsn, "key": key, "model": model}


# ---------------------------------------------------------------------------
# Hand-labeled question sets. Labels = relevant FILES, each verified by
# reading the repo (evidence = the grep/location that pins the label).
# The three question shapes R10 asks for:
#   - exact-symbol:  names a real symbol ("PreparedRequest", "Retry");
#   - conceptual:    describes a capability without symbol names ("how
#                    long to wait before retrying");
#   - cross-file:    truth spans 2+ modules (labels say so).
# ---------------------------------------------------------------------------

REQUESTS_QS = [
    {"q": "Where is PreparedRequest, the final outgoing request, assembled?",
     "files": ["models.py"],
     "evidence": "models.py L378 class PreparedRequest, def prepare L424"},
    {"q": "How are form fields and file uploads encoded in the request body?",
     "files": ["models.py", "utils.py"],
     "evidence": "models RequestEncodingMixin._encode_files; utils._encode_params"},
    {"q": "Where is the HTTP basic auth header constructed?",
     "files": ["auth.py", "utils.py"],
     "evidence": "auth.py _basic_auth_str L34; utils._basic_auth_str caller"},
    {"q": "Where does requests follow redirects and issue the follow-up request?",
     "files": ["sessions.py"],
     "evidence": "sessions.py resolve_redirects L186"},
    {"q": "Which component applies the urllib3 Retry policy on transport errors?",
     "files": ["adapters.py"],
     "evidence": "adapters.py Retry import L34, MaxRetryError usage"},
    {"q": "Where are HTTP status codes mapped to human-readable reasons?",
     "files": ["status_codes.py"],
     "evidence": "status_codes.py _codes mapping module"},
    {"q": "How is proxy configuration resolved from environment variables?",
     "files": ["utils.py"],
     "evidence": "utils.py should_bypass_proxies L810, get_environ_proxies L873"},
    {"q": "What builds the connect and read timeouts passed to urllib3?",
     "files": ["adapters.py"],
     "evidence": "adapters.py HTTPAdapter.send builds urllib3.Timeout"},
]

URLLIB3_QS = [
    {"q": "Where is the pool of reusable HTTP connections implemented?",
     "files": ["connectionpool.py"],
     "evidence": "connectionpool.py L126 class HTTPConnectionPool; _get_conn/_put_conn"},
    {"q": "How does PoolManager route requests to per-host connection pools?",
     "files": ["poolmanager.py"],
     "evidence": "poolmanager.py L164 class PoolManager"},
    {"q": "How does urllib3 decide how long to wait before retrying a failed request?",
     "files": ["util/retry.py"],
     "evidence": "retry.py Retry L41, get_backoff_time L310"},
    {"q": "Where are TLS sockets wrapped and the SSL context applied for https?",
     "files": ["connection.py", "util/ssl_.py"],
     "evidence": "connection.py ssl_wrap_socket usage L674; util/ssl_.py module"},
    {"q": "How is a gzip or deflate compressed response body decoded?",
     "files": ["response.py", "util/response.py"],
     "evidence": "response.py Gzip/DeflateDecoder.decompress L58/L76; util/response.py"},
    {"q": "Where are connect and read timeout values applied?",
     "files": ["util/timeout.py", "connectionpool.py"],
     "evidence": "util/timeout.py class Timeout L25; connectionpool urlopen timeouts"},
    {"q": "How is the request body transformed into chunks for sending?",
     "files": ["connection.py", "util/request.py"],
     "evidence": "connection.py L54 body_to_chunks import, L533 chunk transform"},
    {"q": "How does urllib3 release a connection back to its pool after use?",
     "files": ["connectionpool.py"],
     "evidence": "connectionpool.py _put_conn L300"},
]

QUESTION_SETS = {"requests": REQUESTS_QS, "urllib3": URLLIB3_QS}


# ---------------------------------------------------------------------------
# index passes


def _run_index(project: Path, db: Path, env_extra: dict[str, str]) -> float:
    """`cie index` as a subprocess — the real user path — timed."""
    t0 = time.perf_counter()
    proc = subprocess.run(
        [sys.executable, "-m", "cie.cli", "index", str(project), "--db", str(db)],
        capture_output=True, text=True,
        env={**os.environ, **env_extra},
    )
    if proc.returncode != 0:
        sys.exit(f"cie index failed:\n{proc.stdout}\n{proc.stderr}")
    return time.perf_counter() - t0


def _node_count(project: Path, db: Path) -> int:
    """Node count via the repository (not the CLI — the stats command's
    flag surface isn't the benchmark's contract; the DB is)."""
    import sqlite3

    conn = sqlite3.connect(str(db))
    try:
        return int(conn.execute("SELECT COUNT(*) FROM nodes").fetchone()[0])
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# measurement


def _rel_file(source_file: str, project: Path) -> str:
    try:
        return str(Path(source_file).resolve().relative_to(project))
    except ValueError:
        return source_file


def _ranked_files(rows: list[dict], project: Path) -> list[str]:
    """Dedup the retrieved rows' files, preserving retrieval rank."""
    out: list[str] = []
    for row in rows:
        rel = _rel_file(row.get("source_file") or "", project)
        if rel and rel not in out:
            out.append(rel)
    return out


def _recall_at_k(retrieved: list[str], wanted: list[str], k: int) -> float:
    """File-level recall@k — with 1–2-file labels on small source trees,
    precision@k is structurally capped at |wanted|/k (a perfect system
    that fills all 8 slots gets 0.125); recall@k (all wanted files found
    within the top-k unique retrieved files) is the meaningful bar for
    'where is X implemented' workflows, and MRR complements it."""
    if not wanted:
        return 0.0
    hits = sum(1 for f in retrieved[:k] if f in wanted)
    return round(hits / len(wanted), 3)


def _mrr(retrieved: list[str], wanted: list[str]) -> float:
    """Mean-reciprocal-rank component: 1 / rank of the first relevant
    file, 0.0 when none in the ranked list."""
    for rank, f in enumerate(retrieved, start=1):
        if f in wanted:
            return round(1.0 / rank, 3)
    return 0.0


def _grep_payload_chars(question: str, src_dir: Path) -> int:
    """Naive-side anchor: chars of `grep -rilE <keywords>` output — the
    hit-list file paths only (what a human/agent then has to open and
    read). Char-based by convention, documented in R9."""
    words = [w for w in re.split(r"\W+", question.lower())]
    stop = {"where", "what", "how", "does", "which", "from", "sent", "into",
            "their", "them", "that", "with", "after", "before", "back"}
    words = [w for w in words if len(w) > 3 and w not in stop][:4]
    if not words:
        return 0
    pattern = "|".join(map(re.escape, words))
    proc = subprocess.run(
        ["grep", "-rilE", pattern, str(src_dir)], capture_output=True, text=True,
        timeout=120,
    )
    return len(proc.stdout)


def _context_block(service, question: str):
    """Assemble the actual GraphRAG context block for a question using the
    pure (host-free) layers at the engine level: hybrid retrieval ->
    entity_context expansion -> graphrag._assemble_context. The ToolService
    envelope flattens matches to dicts for MCP transport — the context
    assembler consumes the engine's native HybridMatch objects, so this
    measures the layers directly (identical data, native types)."""
    from cie import graphrag

    engine = service._engine  # noqa: SLF001 - benchmark harness
    matches = engine.hybrid_search(question, top_k=graphrag._RETRIEVE_TOP_K)
    contexts: list[dict] = []
    for match in matches[:graphrag._EXPAND_TOP_N]:
        ctx = engine.entity_context(match.node.label)
        if ctx:
            contexts.append(ctx)
    block = graphrag._assemble_context(matches, contexts)
    return len(block), len(block) // 4  # AI-06's documented chars//4 estimate


def main() -> None:
    ap = argparse.ArgumentParser(
        description="R10's first-party semantic-retrieval benchmark")
    ap.add_argument("--project", required=True, type=Path,
                    help="the corpus repo (must match --corpus's question set)")
    ap.add_argument("--corpus", required=True, choices=sorted(QUESTION_SETS))
    ap.add_argument("--top-k", type=int, default=8, help="unique files judged (default 8)")
    ap.add_argument("--out", type=Path, default=None,
                    help="write the raw JSON here")
    args = ap.parse_args()

    cfg = _require_env()
    project = args.project.resolve()
    questions = QUESTION_SETS[args.corpus]
    k = args.top_k

    from cie.factory import build_tool_service_embedded

    # -- index time, plain vs embeddings (same machine, same corpus) ----
    db_plain = project / ".cie" / "graph-plain.tmp.db"
    db_vec = project / ".cie" / "graph-vec.tmp.db"
    for p in (db_plain, db_vec):
        p.unlink(missing_ok=True)
    baseline_env = {k_: v for k_, v in os.environ.items() if k_ != "CIE_EMBED_DSN"}
    # Two passes each, min taken: the first cold pass eats OS-cache/tree-
    # sitter warmup, biasing whichever runs first — warm-minimum is the
    # honest per-config floor for a wall-clock comparison.
    t_plain = min(_run_index(project, db_plain, baseline_env),
                  _run_index(project, db_plain, baseline_env))
    t_vector = min(_run_index(project, db_vec, {"CIE_EMBED_DSN": cfg["dsn"]}),
                   _run_index(project, db_vec, {"CIE_EMBED_DSN": cfg["dsn"]}))
    n_nodes = _node_count(project, db_vec)

    service = build_tool_service_embedded(project, db_path=db_vec)
    try:
        from cie.embed import embed_text as _probe
        _probe("probe")
    except Exception as exc:  # noqa: BLE001
        sys.exit(f"error: embeddings not reachable through the env fallback: {exc}")

    results = {
        "corpus": args.corpus,
        "project": str(project),
        "k": k,
        "embedding_model": cfg["model"],
        "dsn": cfg["dsn"],
        "index_time": {
            "plain_seconds": round(t_plain, 2),
            "with_embeddings_seconds": round(t_vector, 2),
            "overhead_seconds": round(t_vector - t_plain, 2),
            "nodes": n_nodes,
        },
        "questions": [],
    }
    try:
        results["index_time"]["nodes"] = n_nodes
    except UnboundLocalError:
        pass

    total = {"hybrid_p": 0.0, "semantic_p": 0.0, "hybrid_mrr": 0.0,
             "semantic_mrr": 0.0, "answered": 0}
    for spec in questions:
        hyb_env = service.hybrid_search(spec["q"], top_k=k * 4)
        sem_env = service.semantic_search(spec["q"], top_k=k * 4)
        hyb_rows = hyb_env.get("results", [])
        sem_rows = sem_env.get("results", [])
        hyb_files = _ranked_files(hyb_rows, project)
        sem_files = _ranked_files(sem_rows, project)
        ctx_chars, ctx_tokens = _context_block(service, spec["q"])
        grep_chars = _grep_payload_chars(spec["q"], project)

        row = {
            "q": spec["q"],
            "wanted": spec["files"],
            "label_evidence": spec["evidence"],
            "hybrid_top_files": hyb_files[:k],
            "hybrid_recall_at_k": _recall_at_k(hyb_files, spec["files"], k),
            "hybrid_mrr": _mrr(hyb_files, spec["files"]),
            "semantic_top_files": sem_files[:k],
            "semantic_recall_at_k": _recall_at_k(sem_files, spec["files"], k),
            "semantic_mrr": _mrr(sem_files, spec["files"]),
            "context_chars": ctx_chars,
            "context_tokens_est": ctx_tokens,
            "grep_hitlist_chars": grep_chars,
        }
        results["questions"].append(row)
        total["hybrid_p"] += row["hybrid_recall_at_k"]
        total["semantic_p"] += row["semantic_recall_at_k"]
        total["hybrid_mrr"] += row["hybrid_mrr"]
        total["semantic_mrr"] += row["semantic_mrr"]
        total["answered"] += 1 if (hyb_rows or sem_rows) else 0

    n = len(questions)
    results["summary"] = {
        "mean_hybrid_recall_at_k": round(total["hybrid_p"] / n, 3),
        "mean_semantic_recall_at_k": round(total["semantic_p"] / n, 3),
        "mean_hybrid_mrr": round(total["hybrid_mrr"] / n, 3),
        "mean_semantic_mrr": round(total["semantic_mrr"] / n, 3),
        "questions_answered_with_context": total["answered"],
        "questions_total": n,
        "note": "file-level recall@%d + MRR; dense signal from %s via %s"
                % (k, cfg["model"], cfg["dsn"]),
    }

    print(json.dumps(results, indent=2))
    if args.out:
        args.out.write_text(json.dumps(results, indent=2))
        print(f"raw JSON -> {args.out}", file=sys.stderr)

if __name__ == "__main__":
    main()