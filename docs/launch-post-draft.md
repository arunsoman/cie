# Launch post — DRAFT ONLY (roadmap R20; nothing has been published)

> Drafted 2026-08-31. Publication gates (below) decide if/when this
> ships — **drafting from vibes-free sources only: every claim below is
> footnoted to a measured artifact, and the losses stay in the post**
> (the session-3 critique #10 rule). This file is the claim-audit walk:
> the Count Contract's last walk before an audience sees the numbers.

---

## A. Show HN draft

**Title:** Show HN: Cie — a code-graph MCP server with a task→file→test traceability layer, read-only by default

**Body:**

Hi — I built cie (Code Insight Engine): a local code-graph indexer with a
135-tool MCP surface, a CLI, and an HTTP tool-mount, over an embedded
SQLite graph (zero-config) or Neo4j (teams). It answers "who really calls
close()?"-style questions through the call graph instead of grep, and
keeps a layer nobody else in the category has: task→file→test
traceability with quality-governance tooling.

What I think is different (all measured, sources in the appendix table):

- **Traceability in the graph.** Push your atomic tasks, link them to
  code and tests, and `traceability_chain` walks task→file→test through
  real TESTS edges — the view every code-graph tool I scanned lacks.
- **Honest retrieval.** `callers` returns per-edge provenance
  (`graph` vs `heuristic-name-match`) and a resolution tally, so the
  famous "my heuristic missed 3 of 6 real call sites on psf/requests"
  is a NUMBER IN THE OUTPUT, not a hidden loss. The benchmark docs keep
  their misses.
- **Governance.** Every tool is policy-gated server-side
  (read-only by default; writes 403'd per tool, machine-checked), node
  confidence is persisted, and `confidence_report`/`justification`
  expose why the graph believes a node.
- **Zero-config.** `cie index .` → SQLite; `cie-mcp . --embedded` → stdio
  MCP; `cie init` registers the client; `cie export-html` ships one
  static, network-free HTML of your chains.
- 9 languages via tree-sitter; first-party embedding retrieval measured
  at recall@8 = 1.0 on two third-party corpora (misses published).

It's MIT, single-maintainer, weeks-old code cut to 0.1.0 today — the
release notes keep the maturity caveats I'd want to read before trusting
a tool like this. Benchmarks with reproducible scripts:
docs/benchmarks-requests.md, docs/benchmarks-urllib3.md,
docs/competitor-benchmarks.md.

Repo: https://github.com/kannamma-labs/cie
Install: `pip install "cie[mcp] @ git+https://github.com/kannamma-labs/cie.git"`

*(HN-format note: keep the title ≤80 chars; the body above is the first
comment. No superlatives that aren't footnoted.)*

---

## B. r/LocalLLaMA draft

**Title:** I open-sourced a code-graph MCP server that works offline against your local repos — 135 tools over SQLite/Neo4j, semantic retrieval you can run against any OpenAI-compatible embeddings endpoint

**Body:**

After weeks of dogfooding I cut 0.1.0 of cie. What it does, in the order
I actually use it:

1. `cie index <repo>` — tree-sitter extraction into a local SQLite graph
   (9 languages; Neo4j optional for teams). 665 nodes on psf/requests'
   `src` in ~3s, ~1.2k nodes on urllib3's `src` in ~5s.
2. Point any MCP client at `cie-mcp <repo> --embedded --policy readonly`
   — `callers`, `callees`, `path_between`, `traceability_chain`…
   every answer says HOW it was reached (graph edge vs name heuristic,
   per row) and how complete the call-site resolution is.
3. `cie export-html` — one static HTML with your task→file→test chains;
   opens via `file://`, zero network requests (asserted in tests).
4. Semantic search with YOUR embeddings config — point
   `CIE_EMBED_DSN` at any OpenAI-compatible endpoint (I used NVIDIA NIM
   for the published run; Ollama works the same way). First-party
   benchmark: 16 hand-labeled questions across two well-known repos,
   recall@8 = 1.0, MRR 0.75–0.85 published per tool and per corpus —
   the benchmark script is in the repo, the misses are in the docs.

Honest losses, kept on purpose: the ambiguous-caller heuristic missed
3 of 6 real call sites on psf/requests (28 of 40 on urllib3's bigger
tree) — that number is part of the tool output, not the changelog;
semantic retrieval is measured, the embedding-search competitors I list
are not run head-to-head and the docs say so loudly.

Repo: https://github.com/kannamma-labs/cie (MIT, single maintainer)

---

## C. Claim-by-claim audit table

*(The non-negotiable part, per R20's plan: every sentence that asserts
something must be footnoted to a measured source — file + section + date
+ the commit it was true of. Losses listed WITH wins. If a claim can't
be footnoted, it doesn't go in the post.)*

| # | Claim used in the drafts | Measured source | As of |
|---|---|---|---|
| 1 | `callers`/`callees` per-row `provenance` + `resolution` tallies; "3 of 6 real call sites on psf/requests" miss | docs/benchmarks-requests.md (methodology + reconciliation) · live `callers("close")`: 19/16/3 name-keyed | 2026-08-30 |
| 2 | urllib3: 12 receiver-attributed graph edges vs 28 raw grep matches; 2.24× skeleton compression; **28/40 unresolved call-sites miss** | docs/benchmarks-urllib3.md (script-generated tables) | 2026-08-30 |
| 3 | semantic retrieval recall@8 = 1.0 on 16/16 questions; hybrid MRR 0.854/0.781, semantic 0.754/0.823; misses per question | docs/benchmarks-semantic-{requests,urllib3}.json + docs/competitor-benchmarks.md "Semantic retrieval" (reproduce: scripts/benchmark_semantic.py) | 2026-08-31 (commit 279ee53) |
| 4 | index times: 3.10s plain (665 nodes) / 5.92s (1,207) warm-min | raw JSON artifacts above (machine-local; label as "this pass, not a fleet mean") | 2026-08-31 |
| 5 | 0.87s vs CodeGraphContext 25.55s / Serena 9.39s indexing (forge/, 745 nodes) | docs/competitor-benchmarks.md "Indexing the same codebase" | 2026-08-28 |
| 6 | surface: 135 ToolService tools; conformance 101 verified / 25 graceful / 5 unavailable with reason slugs / 4 backend-down / **0 crashes** | tool-test-lab/surface_results.json (committed artifact; re-run before posting per the Count Contract) | 2026-08-31 |
| 7 | read-only by default: write tools 403'd server-side per surface | tests/test_http_policy.py + docs/security.md threat model | 2026-08-30 |
| 8 | 9 languages | cie/extract.py language tables + tests/test_extract_{c,cpp,csharp}.py | 2026-08-30 |
| 9 | export-html: one static file, zero external references | tests/test_export_html.py + docs/images/ screenshots (recorded via scripts/record_export_html.sh) | 2026-08-30 |
| 10 | task→file→test chains over real TESTS edges; orphans listed | `traceability_chain`/`traceability_orphans` tools (conformance-verified) + export-html views | 2026-08-31 |
| 11 | "0.1.0 stable" | tag `v0.1.0` + GitHub release (a22b4bf); PyPI name decision pending — **posts must say install-from-GitHub**, matching README | 2026-08-31 |
| 12 | 292 tests | `pytest -q` (re-run on the publishing commit; the badge/citation is refreshed per release, R3's convention) | 2026-08-31 |
| 13 | competitor language/tool counts and star counts (if mentioned) | docs/competitive-landscape.md table + docs/competitive-delta-2026-08-30.md, **vendor counts stay labeled vendor-claims** | 2026-08-30 |
| 14 | "codebase-memory-mcp / others: 120× fewer tokens"-style numbers, if used at all | **vendor claims only — cite as such or drop** (R9's convention) | 2026-08-30 |

## D. Publication gates (the checklist to run before anything ships)

1. [x] R6 shipped — v0.1.0 released 2026-08-31 (tag + GitHub release,
       caveats block kept in the notes).
2. [x] R9's numbers public and script-reproducible
       (scripts/benchmark.sh + docs/benchmarks-urllib3.md, third dataset
       with its own honest-loss section).
3. [x] Claim table (§C) complete — every row footnoted; no vibes.
4. [ ] Re-run §C's measurable rows on the publishing commit (3, 6, 12
       at minimum — the Count Contract walk) and update the "as of"
       column; if any number moved, the draft text moves with a dated
       note, never silently.
5. [ ] Owner's explicit go-ahead to publish.
6. [ ] Show HN timing re-check (weekday morning UTC, no rate-limit/
       ban-window concerns) — mechanics, not content.
7. [ ] The losses stay in: #2's misses, #3's misses, #14's labeling rule.

## Provenance

- Drafted per todo.md R20 (which requires the draft + this audit table;
  publishing stays gated per the item's own rules).
- Sources of record: the four benchmark docs, the committed conformance
  JSON, and the test suite — same files the README cites on this commit.