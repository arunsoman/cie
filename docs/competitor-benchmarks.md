# Competitor benchmarks

**Run:** 2026-08-28, one pass, one target codebase, real installs of real
competitor tools (not estimated from their marketing numbers) — companion
to `docs/benchmarks.md` (cie's own numbers in isolation) and
`docs/competitive-landscape.md` (the feature-level comparison this
measures a slice of). Treat this as a first real data point, not a mature
benchmark suite.

## Methodology

**Target:** the same codebase as `docs/benchmarks.md` — `forge/` from
protobox's `be-v2` backend, 36 Python files, 12,366 lines.

**Tools actually installed and run, not simulated:**

| Tool | Install | Version/commit |
|---|---|---|
| cie | this repo | commit at time of run |
| [CodeGraphContext](https://github.com/CodeGraphContext/CodeGraphContext) | `pip install codegraphcontext`, `--db kuzudb` (embedded, zero-config — the fairest comparison to cie's own embedded backend) | latest on PyPI, 2026-08-28 |
| [Serena](https://github.com/oraios/serena) | `uvx --from git+https://github.com/oraios/serena serena ...` | HEAD (`7fcbca7e`), 2026-08-28 |

CodeGraph (the 47k-star category leader) was not run — it's a Node/
TypeScript project without a documented single-command CLI indexing
entrypoint suitable for this harness in the time available; not included
rather than estimated.

## Indexing the same codebase

| Tool | Time | Output |
|---|---|---|
| **cie** (`cie index`) | **0.87s** | 745 nodes, 1,656 edges (910 resolved `calls`) |
| CodeGraphContext (`cgc index --db kuzudb`) | 25.55s | 622 functions, 83 classes, 1,868 `CALLS` edges (174 skipped as `ambiguous_function_target`) |
| Serena (`serena project create --index`) | 9.39s | LSP symbol cache (no node/edge counts reported — different indexing model, see below) |

cie indexed the same codebase roughly **29x faster than CodeGraphContext**
and **11x faster than Serena** in this run. Read this with the
methodology caveats below, not as a universal claim — see "What this
doesn't control for."

## Query tasks

### Task 1: find the definition of `AgentResult`

| Tool | Result | Fresh-process latency |
|---|---|---|
| cie | 1 match, correct | 0.40s |
| CodeGraphContext | 1 match, correct | 1.43s |
| Serena | not measured — no CLI query command exists (see below) |

Both cie and CodeGraphContext got this right, at comparable per-call
latency once you account for CGC's Python/Typer CLI startup overhead
(the same class of cost cie's own CLI would show if it had a
per-query CLI command — it doesn't yet, only `cie-mcp`'s persistent
server, which is why this column is a fresh-process comparison, not a
warm-session one; see the note below).

### Task 2: find every real caller of `_post` — the interesting one

Ground truth, verified by direct `grep`: **126 real call sites**, split
across **two distinct methods** both named `_post` (`tools.py::_post`,
called 121 times, and `reporter.py::_post`, called 5 times) — a
genuinely ambiguous bare name, deliberately chosen for this benchmark
because Task 1 wasn't testing anything hard.

| Tool | Reported (at time of this benchmark) | What actually happened |
|---|---|---|
| CodeGraphContext | 20+ (display-truncated) | Returned real callers spanning BOTH `_post` definitions — conflates the two targets, but every result shown was a real call site |
| **cie (before the fix below)** | **2** | Also conflates the two targets, but returned an inconsistent partial subset (1 caller of each) — a real, diagnosed limitation, not the resolver being broken (see below) |
| **cie (after the fix below)** | **126, correctly** (30 shown at the default `limit`, `truncated: true`) | Aggregates across every exact-name match, matching (and slightly beating, since it distinguishes exact from substring matches) CodeGraphContext's own approach |

**This sent us digging, and it was worth it — the finding was more precise
and less flattering to cie than "cie is worse here," and it led straight
to a real, now-fixed bug:**

Ground-truthed `cie.callgraph.resolve_call_edges` directly (not through
`ToolService.callers()`) against this exact codebase: it resolved **all
126 of 126** real call sites correctly, correctly split 121/5 across the
two distinct `_post` node ids. **The pass-2 call-graph resolver itself was
exactly right — verified against the actual grep ground truth, not just
"looks plausible."**

The bug was one layer up, in bare-name symbol resolution
(`_resolve_symbol_id` in `cie/in_memory_repository.py`, and the identical
`LIMIT 1`-based pattern in `cie/neo4j_repository.py`): given an ambiguous
name matching two distinct definitions, it picked exactly one and
`get_callers`/`get_callees`/`test_map`/`actual_callers` then only ever
saw edges touching that one node — silently, with no indication other
definitions existed. **Fixed** (same day, both backends): a new
`_resolve_symbol_ids` returns every EXACT-name match (falling back to the
old single best-substring-match behavior only when there is no exact
match at all, so an unambiguous name's behavior is unchanged), and all
four consumer methods now aggregate across every resolved id instead of
just the first. Re-run against this exact codebase post-fix: `callers("_post")`
now correctly reports 126 total callers (30 shown at the default limit,
`truncated: true`) — see `tests/test_embedded_repository.py`'s
`test_get_callers_aggregates_across_every_ambiguous_definition` and
neighboring tests for the regression coverage.

### Task 3: understand a file (`file_skeleton`)

Not re-run against competitors here — see `docs/benchmarks.md` for cie's
own number in isolation (a real loss for cie on this specific file: the
skeleton response was larger than the raw source). CodeGraphContext has
no direct equivalent single command; a fair comparison would need its
`analyze complexity`/`find type` combination, which isn't the same shape
of query. Left for a follow-up pass rather than forcing a mismatched
comparison.

## What this doesn't control for

- **One codebase, one pass, no repeats** — no variance/error bars.
- **CodeGraphContext's 174 skipped calls** ("ambiguous_function_target")
  are a deliberate precision choice on its part — it may be trading
  recall for confidence in a way this comparison doesn't score either
  way.
- **Indexing methodology differs**: cie and CodeGraphContext both build a
  persistent, queryable graph up front; Serena indexes an LSP symbol
  cache and answers queries by talking to a live language server
  per-request — different architectures with different tradeoffs, not
  directly comparable on "index time" alone.
- **No CLI query interface for Serena** in the time available for this
  pass — it's designed to be driven by an MCP client, not scripted
  directly the way CodeGraphContext's `cgc find`/`cgc analyze` or cie's
  Python API can be. Its query-latency numbers are missing here, not
  zero or bad.
- **CodeGraph (the actual 47k-star leader) is absent** — see above.
- Only one language (Python), one codebase size (~12K lines). No claim
  this generalizes.

## Reproducing this

```bash
# cie
pip install "cie-mcp[mcp]"
cie index /path/to/repo

# CodeGraphContext
pip install codegraphcontext
cgc --db kuzudb --db-path /tmp/bench.kuzu index /path/to/repo --summarize

# Serena
uvx --from git+https://github.com/oraios/serena serena project create /path/to/repo --index
```

Time each with `time`, then compare — same as this pass.

---

## Semantic retrieval (GraphRAG) — first-party, 2026-08-31

**What's new this pass:** R10's semantic layer now runs first-party —
`cie.embed`'s env-gated OpenAI-compatible fallback
(`CIE_EMBED_DSN` + key; stdlib, no new dependency — see
`cie/embed.py`'s tier contract) makes the embedding pipeline reproducible
from this repo alone. This section measures **cie's own semantic
retrieval** (the layer R5 left host-gated, now standalone-capable); the
competitor retrieval stacks below are **vendor-documented configs, not
runs** — running them requires their services/indices, which the
measurement environment does not have. Labeled as such; nothing here
claims a head-to-head with them.

### What was measured (ours — measured)

`scripts/benchmark_semantic.py` (raw JSON committed alongside the doc):
16 hand-labeled questions — 8 per corpus, spanning exact-symbol,
conceptual (no symbol names in the query), and cross-file (truth spans
2+ modules) — on the same corpora as R9's benchmarks, hand-labeled
by reading the repos, grep evidence recorded per label in the script.

| Corpus (pinned) | Nodes | Index time plain | + embeddings | Recall@8, hybrid | Recall@8, semantic | MRR hybrid | MRR semantic |
|---|---|---|---|---|---|---|---|
| psf/requests @ `5460f467` (`src/requests`) | 665 | 3.10s | 3.13s | **8/8 = 1.0** | **8/8 = 1.0** | **0.854** | 0.754 |
| urllib3 @ `85a8a9cf` (`src/urllib3`) | 1,207 | 5.92s | 4.82s | **8/8 = 1.0** | **8/8 = 1.0** | 0.781 | **0.823** |

- Every one of the 16 questions found its full relevant-file set within
  the top 8 unique retrieved files — including the conceptual ones
  ("how long to wait before retrying" → `util/retry.py` without naming
  Retry) and every cross-file label ("TLS wrapping" → connection.py +
  util/ssl_.py).
- **Ranking (MRR) is where retrievers differ, and neither dominates:**
  hybrid's 3-signal fusion wins on requests (0.854 vs 0.754); dense-only
  wins on urllib3 (0.823 vs 0.781). Hybrid's floor is the higher of the
  two (0.781 vs semantic's 0.754) — its worst case is better.
- **Embedding index overhead at this scale is ~free**: warm-min index
  time was 665 nodes in +0.03s and 1,207 nodes in −1.1s (i.e. inside
  run-to-run noise — the batched, bounded-concurrency client sends
  ~one request per 16 nodes).
- Context assembly (the R10-decoupled pure layer) produced 776 avg
  tokens (requests) / 1,167 avg tokens (urllib3) per question —
  full context blocks with callers/tests expansion, compared against a
  naive `grep -rilE` hit-list of 161/349 est. tokens — different
  deliverables (assembled context vs a filename list), published as a
  calibration point, not a win.

**Honest misses (published as found):**
- The hybrid retriever's lexical+graph biases are visible in specific
  misses: "release a connection back to its pool" (urllib3) ranks
  `connectionpool.py` 2nd via hybrid (MRR 0.5) vs 4th via dense (0.25);
  the retry-backoff question's reverse (hybrid 0.25, dense 1.0) —
  query shape decides which signal dominates, and the answer set does
  not hide which one fired (every `hybrid_search` row carries
  `lexical_score`/`dense_score`/`graph_score`; edge-level provenance
  from R7).
- A **full-repo index (repo root, tests/docs included)** was also run
  for requests before scoping to the source tree: file-level precision
  collapses to ~0.14 mean because test-function labels dominate
  embedding retrieval. Scope the index to the source tree (R9's corpus
  convention, what `--src-glob` drives) — or read the miss here rather
  than rediscover it.

### Configuration, stated (theirs — vendor-documented, NOT run here)

| Retrieval stack | Stars / version | Indexing config (per their docs, 2026-08-31) | Run here? |
|---|---|---|---|
| **cie** (this run) | — | `cie index` (`sqlite` embedded, 665/1,207 nodes) → hybrid = lexical + dense (cosine, `node_embedding_text` payload: label/kind/decl line + docstring) + graph degree; dense via `nvidia/nemotron-3-embed-1b` (dim 2048) through the env-gated OpenAI-compatible fallback, `input_type` query-vs-passage at call sites | **measured, 2026-08-31** |
| [claude-context](https://github.com/zilliztech/claude-context) | 12,455 | per README (as of 2026-07-14 commit): Milvus/Zilliz cloud vector DB; embeddings via user-supplied OpenAI/voyage key; MCP `index_codebase`/`search_code` tools — **vendor-documented** | not run here (needs their vector service + API keys) |
| [grepai](https://github.com/yoanbernabeu/grepai) | 1,827, v0.36.0 (2026-08-30) | per README: 100% local — Ollama `nomic-embed-text` (default), LM Studio or OpenAI supported; `grepai watch` indexing daemon; call-graphs alongside vectors — **vendor-documented** | not run here (needs a local Ollama stack) |

The `120× fewer tokens` style claims from codebase-memory-mcp remain
**vendor claims** (see R9's note); everything in the measured table
above is this repo's own run, reproducible below.

### Reproducing

```bash
CIE_EMBED_DSN=https://integrate.api.nvidia.com/v1 \
CIE_EMBED_API_KEY=... \
CIE_EMBED_MODEL=nvidia/nemotron-3-embed-1b \
python scripts/benchmark_semantic.py \
    --project /tmp/requests/src/requests --corpus requests --out /tmp/sem-requests.json

# second corpus
CIE_EMBED_DSN=https://integrate.api.nvidia.com/v1 \
CIE_EMBED_MODEL=nvidia/nemotron-3-embed-1b \
python scripts/benchmark_semantic.py \
    --project /tmp/urllib3-bench/src/urllib3 --corpus urllib3 --out /tmp/sem-urllib3.json
```

(Repos: `git clone https://github.com/psf/requests` and
`git clone https://github.com/urllib3/urllib3`, checkout
`5460f467` / `85a8a9cf` — the same pins as `docs/benchmarks-*.md`.)
