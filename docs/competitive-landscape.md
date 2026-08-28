# Competitive landscape

**Last researched:** 2026-08-28. This is a snapshot, not a living
benchmark — the category (code-graph / code-intelligence engines for AI
coding agents) is moving fast; re-verify claims before citing them
somewhere that matters.

## TL;DR

cie's nearest neighbors are **CodeGraphContext** (the only other
Neo4j-backed, pluggable-graph-backend tool in the category) and
**CodeGraph** (the category's adoption leader, 47.4k stars, embedded
SQLite). cie's real differentiation isn't "better code search" — every
competitor here is optimized purely for that one job. It's scope: cie is
the only one that fuses a code graph, task/QA traceability, and
continuous quality governance into one system with a genuinely
open-ended language-extension model. The most significant gap, and the
single highest-leverage thing to fix, is that **cie does not speak the
Model Context Protocol** — every competitor listed here does.

## Comparison

| | **cie** | **CodeGraphContext** | **CodeGraph** | **Serena** |
|---|---|---|---|---|
| Graph backend | Neo4j | Neo4j / FalkorDB / KuzuDB / LadybugDB (pluggable) | Embedded SQLite + FTS5 | None — wraps LSP servers live |
| Adoption | New, 0 stars | Community project | 47.4k stars in 5 months — category leader | 25.2k stars, 170+ contributors |
| Languages out-of-box | 4 (tree-sitter) | 23 (tree-sitter/SCIP) | 21 (tree-sitter) | 40+ (via LSP) |
| Extending to a new language | Register any `LanguageAdapter` — no grammar or LSP required | Needs a tree-sitter grammar or SCIP indexer | Needs a tree-sitter grammar | Needs a working LSP server |
| Tool count | ~123 | 14 | not disclosed; MCP-native | many, LSP-backed |
| Task/QA tracking | Yes — AtomicTask/QA CRUD, traceability chains | No | No | No |
| Quality/drift/test intelligence | Yes — clone detection, drift detection, confidence scoring, contracts/invariants, state-machine validation, tech-debt reports | No (has `manage_adr`, not the same thing) | No | No |
| Per-agent tool policy | Yes — `ToolPolicy`/`WRITE_TOOLS`, server-enforced | No (left to the MCP client) | No | No |
| Speculative-vs-canonical graph state tied to git commits | Yes — promote/revert/ast-delta | `detect_changes` (live sync only) | file-watcher sync only | No (LSP is always live) |
| MCP protocol | **No** — own ToolService + HTTP convention | Yes | Yes | Yes |

Runners-up not detailed above, for context: **GitNexus** (41,958 stars,
zero-server browser-based graph, 16 MCP tools + resources + skills — the
most "batteries-included" MCP integration in the category) and
**claude-context** / **grepai** (dedicated, independently-benchmarked
embedding/semantic search — a narrower but more mature slice than cie's
own GraphRAG layer).

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
   query.

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

## Where competitors are genuinely ahead

Honest gaps, not hedged:

- **No MCP protocol implementation.** Every competitor found speaks the
  actual Model Context Protocol; cie speaks its own `ToolService`/HTTP
  convention. This is the single biggest concrete gap — without an MCP
  adapter, cie can't plug into Claude Code, Cursor, or any other MCP host
  the turnkey way these competitors do.
- **Out-of-box language breadth.** cie ships 4 tree-sitter languages
  today vs. 21–40+ for every competitor. The architecture doesn't cap
  language support, but shipped coverage is far behind.
- **Adoption and battle-testing.** CodeGraph and Serena have tens of
  thousands of users and independently-verified performance benchmarks
  (token/cost reduction numbers). cie has zero external users and no
  benchmarks run against it — any performance claim here would be
  unverified.
- **Setup friction.** CodeGraph's single-SQLite-file, no-server design is
  far lower-friction than cie's Neo4j requirement — a real cost for
  casual/local use in exchange for the multi-hop graph query power
  SQLite+FTS5 doesn't have.
- **Semantic/vector search maturity.** claude-context and grepai have
  dedicated, independently-benchmarked embedding retrieval; cie's
  GraphRAG/embedding layer (`cie/graphrag.py`, `cie/embed.py`) exists but
  is unbenchmarked.

## If prioritizing one next move for competitiveness

An MCP protocol adapter over the existing `ToolService`/`tool_schema`/
`tool_policy` surface (see `cie/tool_schema.py`, `cie/tool_policy.py`) —
the tool definitions and per-agent authorization already exist in a
shape close to what MCP needs; the gap is the wire protocol, not the
underlying capability.

## Sources

- [Ry Walker — Code Intelligence Tools for AI Agents Compared](https://rywalker.com/research/code-intelligence-tools)
- [Ry Walker — CodeGraph](https://rywalker.com/research/codegraph)
- [CodeGraphContext (GitHub)](https://github.com/CodeGraphContext/CodeGraphContext)
- [colbymchenry/codegraph (GitHub)](https://github.com/colbymchenry/codegraph)
- [oraios/serena (GitHub)](https://github.com/oraios/serena)
- [eas4ai/code-graph-mcp (GitHub)](https://github.com/eas4ai/code-graph-mcp)
- [Sourcegraph — AI Coding Context Tools Compared](https://sourcegraph.com/resources/context-compare)
