# Competitive landscape

**Last researched:** 2026-08-30 (numbers refreshed live via the
competitive-delta agent; full per-competitor reasoning in
[`docs/competitive-delta-2026-08-30.md`](competitive-delta-2026-08-30.md)).
This is a snapshot, not a living
benchmark — the category (code-graph / code-intelligence engines for AI
coding agents) is moving fast; re-verify claims before citing them
somewhere that matters.

## TL;DR

cie's nearest neighbors are **CodeGraphContext** (the only other
Neo4j-backed, pluggable-graph-backend tool in the category) and
**CodeGraph** (the category's adoption leader, 68.7k stars, embedded
SQLite). Since the 08-28 snapshot three MCP-native code-graph entries
outscale CodeGraphContext: **Graphify** (112.5k stars — code + docs +
SQL + PDFs into one graph, shareable HTML artifact),
**codebase-memory-mcp** (41.2k — arXiv-paper'd KG server, 162 languages
[vendor claim]) and **code-review-graph** (31.0k — local-first, published
reproducible benchmarks); all remain pure retrieval/navigation [scan:
`.cie/competitive/snapshot-2026-08-30.json`]. cie's real differentiation isn't "better code search" — every
competitor here is optimized purely for that one job. It's scope: cie is
the only one that fuses a code graph, task/QA traceability, and
continuous quality governance into one system with a genuinely
open-ended language-extension model.

## Where cie genuinely excels

1. **Extends to any language, not just ones with existing tooling.**
   Every competitor is bounded by "does a tree-sitter grammar or LSP
   server exist for this language." cie's `LanguageAdapter` registry has
   no such requirement — proven by wiring in `nirdosha`, a brand-new
   custom language with neither a tree-sitter grammar nor an LSP, just by
   wrapping its own compiler's AST dump (`emit-ast`). No competitor
   surveyed claims this.

2. **Task/PRD traceability lives in the same graph as the code.** None of
   the surveyed tools track tasks, QA cycles, or spec-to-file-to-test
   chains — they're pure retrieval/navigation tools. cie can answer
   "which files implement this task, and are they tested" as one graph
   query — over Neo4j or the zero-config embedded SQLite backend alike
   (`cie.embedded_task_repository.EmbeddedTaskRepository`).

3. **A real quality-governance layer, not just retrieval.** Clone
   detection, drift detection (code vs. spec/architecture), per-code-node
   confidence scoring with recorded agent verdicts, contracts/invariants,
   state-machine extraction and validation, tech-debt reporting. This is
   a fundamentally different value proposition — continuous codebase
   auditing, not "answer one question."

4. **Server-enforced per-agent-type authorization.** Every competitor
   leaves write-permission entirely to the MCP client's own settings.
   cie's `WRITE_TOOLS` classification and `ToolPolicy` are enforced in the
   tool surface itself — a read-only agent literally cannot see write
   tools in its schema list, regardless of which client connects it.

5. **Git-aware speculative graph state.** Promote/revert/ast-delta against
   a commit-linked speculative-vs-canonical graph — nothing else surveyed
   has an equivalent to "propose a graph change, then commit or discard
   it" mapped onto real git semantics.

6. **Speaks real MCP, with a zero-config path.** Closed since this table
   was first drafted: a real MCP server
   (`cie-mcp`) and an embedded SQLite backend needing no Neo4j setup — the
   comparison below is left as originally researched, but MCP and setup
   friction are no longer open gaps the way rows 5–6 of the table still
   describe them.

7. **7 tree-sitter languages out of the box (R12: C added).** Go and Rust
   added — function/method
   extraction, signatures, and receiver/impl-method call resolution, each
   verified against a real parse, not assumed from grammar docs. Still
   well behind the 21–40+ every competitor here ships (row below), and
   two things are an honest, documented gap for these two specifically:
   no import-edge extraction, no docstring extraction (`cie/extract.py`'s
   module docstring).

8. **135 tools vs. 14 (CodeGraphContext) doesn't cost selection
   accuracy — measured, not assumed.** The obvious worry about a large,
   specific tool surface is that an agent picks the wrong one more
   often; `docs/tool-selection-accuracy.md` tested that directly (14
   tasks, deliberately including cie's own confusable near-duplicates —
   5 "coverage"-named tools, `callers` vs `actual_callers`) against the
   full read-only surface measured at the 08-30 snapshot (81 tools then;
   85 read-only today under `inspector` — labels per README's convention)
   and a 14-tool subset. Result: 14/14
   correct, both conditions, one run. Real caveats apply (N=1, ceiling
   effect, tool-name-only — see the doc), but the number-of-tools table
   row below is now backed by evidence that breadth is a capability
   edge here, not just a marketing liability to manage around.

## Comparison

*(Context for the claims above, not the headline — read those first.)*

| | **cie** | **CodeGraphContext** | **CodeGraph** | **Serena** |
|---|---|---|---|---|
| Graph backend | Neo4j, or embedded SQLite (zero-config) | Neo4j / FalkorDB / KuzuDB / LadybugDB (pluggable) | Embedded SQLite + FTS5 | None — wraps LSP servers live |
| Adoption | New, 0 stars | Community project | 68.7k stars — category leader | 28.6k stars, 170+ contributors |
| Languages out-of-box | 6 (tree-sitter: Python/JS/TS/Java/Go/Rust) | 23 (tree-sitter/SCIP) | 21 (tree-sitter) | 40+ (via LSP) |
| Extending to a new language | Register any `LanguageAdapter` — no grammar or LSP required | Needs a tree-sitter grammar or SCIP indexer | Needs a tree-sitter grammar | Needs a working LSP server |
| Tool count | 135 | 14 | not disclosed; MCP-native | many, LSP-backed |
| Task/QA tracking | Yes — AtomicTask/QA CRUD, traceability chains (Neo4j or zero-config SQLite) | No | No | No |
| Quality/drift/test intelligence | Yes — clone detection, drift detection, confidence scoring, contracts/invariants, state-machine validation, tech-debt reports | No (has `manage_adr`, not the same thing) | No | No |
| Per-agent tool policy | Yes — `ToolPolicy`/`WRITE_TOOLS`, server-enforced | No (left to the MCP client) | No | No |
| Speculative-vs-canonical graph state tied to git commits | Yes — promote/revert/ast-delta | `detect_changes` (live sync only) | file-watcher sync only | No (LSP is always live) |
| MCP protocol | Yes — `cie-mcp` (real `mcp` SDK) | Yes | Yes | Yes |

Runners-up not detailed above, for context: **GitNexus** (46.5k stars,
zero-server browser-based graph, 16 MCP tools + resources + skills — the
most "batteries-included" MCP integration in the category) and
**claude-context** (12.5k) / **grepai** (1.8k) (dedicated,
independently-benchmarked embedding/semantic search — a narrower but more
mature slice than cie's own GraphRAG layer).

## Where competitors are genuinely ahead

Honest gaps, not hedged:

- **Out-of-box language breadth.** cie ships 7 tree-sitter languages
  today (Python/JS/TS/Java/Go/Rust) vs. 21–40+ for every competitor. The
  architecture doesn't cap language support, but shipped coverage is
  still far behind — narrowed, but not closed, by the recent Go/Rust
  additions.
- **Adoption and battle-testing.** CodeGraph and Serena have tens of
  thousands of users and independently-verified performance benchmarks
  (token/cost reduction numbers). cie has zero external users and two
  small first-pass benchmarks (`docs/benchmarks.md`,
  `docs/benchmarks-requests.md` — the second added specifically because
  the first's proof case was one non-public repo) — real, and now on one
  well-known public repo too, but still two datasets, nowhere near
  CodeGraph's 7-repo published range or Serena's scale of verification.
- **Semantic/vector search maturity.** claude-context and grepai have
  dedicated, independently-benchmarked embedding retrieval; cie's
  GraphRAG/embedding layer (`cie/graphrag.py`, `cie/embed.py`) exists but
  is unbenchmarked.
- **Maturity signals.** Alpha (`0.1.0a2`), single-author commit history —
  real gaps, not just optics. (Test-suite depth specifically improved:
  4 files/38 tests → 10 files/99 tests, covering task/QA,
  quality-governance, and the language-adapter registry areas that had
  zero coverage before — narrowed, not the same gap it started as.)

## If prioritizing one next move for competitiveness

*(Updated 2026-08-29 — the three items this section previously pointed
to — the MCP adapter, the task/QA-traceability Neo4j gate, and the
4-tree-sitter-language ceiling — have each been at least partly
addressed; re-prioritized against what's still open.)* Out-of-box
language breadth (7 tree-sitter languages vs. 21–40+ for every
competitor here) is still the largest remaining gap in this table
specifically. The single highest-leverage item overall,
though, is maturity (workstream C): an alpha days old with a single
author and thin test coverage is what makes every other row in this
table harder to trust at face value, competitive or not.

## Sources

- [Ry Walker — Code Intelligence Tools for AI Agents Compared](https://rywalker.com/research/code-intelligence-tools)
- [Ry Walker — CodeGraph](https://rywalker.com/research/codegraph)
- [CodeGraphContext (GitHub)](https://github.com/CodeGraphContext/CodeGraphContext)
- [colbymchenry/codegraph (GitHub)](https://github.com/colbymchenry/codegraph)
- [oraios/serena (GitHub)](https://github.com/oraios/serena)
- [eas4ai/code-graph-mcp (GitHub)](https://github.com/eas4ai/code-graph-mcp)
- [Sourcegraph — AI Coding Context Tools Compared](https://sourcegraph.com/resources/context-compare)
