# Competitive delta — 2026-08-30

**Generated:** 2026-08-30 (snapshot `.cie/competitive/snapshot-2026-08-30.json`,
`generated_at` 2026-08-30T14:31:13+00:00, live GitHub API) · **cie:** 0.1.0a3,
126 tools (43 write / 83 read), suite 171. **Scan coverage:** 14/14
registered competitors fetched, no per-repo errors; 11 discovery
candidates. All README-derived capability lines below are **vendor
claims** unless marked verified-here.

**`⚠ DIFFERENTIATOR IMPACT`: none killed this scan.** No competitor or
candidate shows task/QA/PRD traceability in a code graph, a
quality-governance layer (clone/drift/confidence/contracts), or git-tied
speculative-vs-canonical state. Two proximity signals — see WATCH items
W1, W2.

## Verified numbers (live, from snapshot)

Tool | Stars | Note
---|---|---
Graphify | 112,510 | biggest in category; `/graphify` skill, EXTRACTED/INFERRED edge tags, graph.html artifact
CodeGraph | 68,671 | leader; announcing hosted platform + PR test-impact (see W1)
GitNexus | 46,463 | zero-server browser graph; agent skills + hooks + AGENTS.md/CLAUDE.md on `analyze`
codebase-memory-mcp | 41,239 | arXiv paper; 162 languages (vendor claim); 3D graph UI at localhost:9749
code-review-graph | 31,014 | reproducible published benchmarks
Serena | 28,645 | LSP-backed; agent-first abstractions; 28.6k
CodeGraphContext | 4,137 | only other pluggable-graph-backend
code-graph-rag | 4,850 | Memgraph; **runtime overlay: merges test-run/eBPF traces into the graph**
SocratiCode | 3,277 | 2.45M-LOC benchmark (61% less context — vendor claim); **sibling MCP-policy-proxy project**
gortex | 1,507 | 257 languages (claim); cross-repo graphs; 20 agent integrations; Web UI
claude-context | 12,455 | embedding search slice [adjacent]
grepai | 1,827 | 100% local semantic+call-graphs [adjacent]
codeseek | 765 | call graphs + hybrid search [adjacent]

## Per-competitor delta (cie verdicts)

- **CodeGraph — BEHIND (adoption), AHEAD (scope).** Auto-sync on change +
  one-line installer + "already installed? upgrade" pipeline is unmatched
  among registered competitors. Still no governance/traceability [scan].
- **Graphify — PARITY (graph build), BEHIND (they have the shareable
  artifact).** Free fully-local tree-sitter code maps with
  every-edge-provenance tagging plus a clickable `graph.html` — that last
  piece is exactly the "quotable/shareable" slot the goal doc wants.
  cie's equivalent: none shipped (demo.svg is static); `confidencereport`/
  `record_verdict` cover node-level provenance, not per-edge tagging.
- **GitNexus — BEHIND (integration depth).** `analyze` installs skills,
  registers Claude Code hooks, writes AGENTS.md/CLAUDE.md context files
  [scan]. cie: `install_git_hook` exists on the tool surface but nothing
  comparable to their write-the-context-files onboarding.
- **codebase-memory-mcp — BEHIND (they have research credibility + 162
  languages; vendor-claimed), AHEAD (scope).** arXiv paper + 31-repo eval
  [vendor claim]. 15 tools vs cie's 126 but narrower + faster [vendor
  claim]. Also indexes Dockerfiles/K8s as graph nodes — cie doesn't.
- **code-review-graph — PARITY (retrieval), BEHIND (benchmark
  reproducibility).** Their "Reproducing the benchmarks" section is the
  practice cie invented for itself in docs/ but published for only 2 repos.
- **Serena — AHEAD (scope: no governance), BEHIND (maturity/UX polish —
  "end users are agents" doc stance, 170+ contributors [vendor claim]).**
- **CodeGraphContext — AHEAD (surface: 126 tools vs 14).** Nearest
  philosophical neighbor (pluggable backends, incl. Neo4j). No traceability either.
- **code-graph-rag — AHEAD (they can't trace tasks) / BEHIND (they can
  see runtime).** Test-run/eBPF trace overlay into a static graph is a
  capability cie has no equivalent for; their AST surgical patching with
  diff-preview overlaps cie's `propose_patch`/`apply_patch`, with a
  preview UI [vendor claim].
- **SocratiCode — AHEAD (governance breadth) / WATCH (policy).** Ships
  tool-governance as a SEPARATE sibling product (local-first MCP policy
  proxy: tool blocking, SQL-mutation control [vendor claim]). cie's
  `ToolPolicy` is in-process and enforced at dispatch (now incl. the HTTP
  surface, this session) — differentiator intact; the ecosystem now has a
  named standalone-player for that slice.
- **gortex — AHEAD (traceability is unique to cie), BEHIND (language
  count 6 vs 257 [vendor claim], Web UI, cross-repo).**
- **claude-context / grepai / codeseek [adjacent] — AHEAD (graph vs no
  graph); their embedded-retrieval benchmarks remain unbenchmarked
  counterparts of cie's own GraphRAG layer (see SHOULD S3).**

## New-entrant triage (11 candidates, 2 registered-bar checks pass)

- `dosco/graphjin` (3.2k★) — GraphQL/MCP over databases+APIs; **not a code
  graph** → ignore.
- `justrach/codedb` (1.4k★) — MCP-native, ≥1k★, capability overlap
  UNVERIFIED this scan → **watchlist**, check README next scan.
- `harshkedia177/axon` (807★), `aovestdipaperino/tokensave` (605★) —
  graph-MCP / broad-tool MCP servers; below or at bar → watchlist.
- `jpicklyk/task-orchestrator` (205★) — task orchestration MCP, **not
  code-graph-integrated**; tracked only because integrated task tooling
  is differentiator #1's neighborhood.
- `Cranot/roam-code` (510★), `ozgurcd/gograph` (212★), `bartolli/codanna`
  (729★), `justrach/*` below-bar remainder, `win4r/codebase-memory-mcp-pro`
  (fork-adjacent, 222★), `GlitterKill/sdl-mcp` (467★) — ignore.

## What we MUST add

**None this scan.** No verified differentiator-killer, and this morning's
HTTP-policy fix (routes `ToolPolicy` enforcement, 16 new tests) already
closed the only trust hole found in this session. Stating this honestly
per the repo's own bar — do not manufacture MUSTs.

## What we SHOULD add (prioritized, effort, first verification step)

- **S1 — Shareable graph artifact** (Graphify's `graph.html` is the
  exemplar): one command (e.g. `cie export-html`) producing a
  self-contained, read-only HTML of a project's graph **centered on
  task→file→test traceability chains** — the view nobody else can render.
  Effort M·S (static export first, no server, no auth surface). Verify:
  export a real clone (psf/requests) and open it with zero deps; put the
  screenshot where demo.svg lives.
- **S2 — `cie init` one-command agent onboarding** (CodeGraph's installer
  + GitNexus's AGENTS.md/hooks exemplars): detect MCP clients, register
  the stdio server, write context files. Effort M. Verify: fresh clone →
  `cie init` → Claude Code lists cie tools, zero manual config.
- **S3 — Third independent benchmark + reproduction script** (code-review
  graph's "Reproducing the benchmarks" exemplar): extend
  docs/benchmarks-requests.md to a scripted harness over a 3rd repo, with
  the honest-loss section kept. Effort M. Verify: `scripts/` entry runs
  end-to-end and both docs cite it.
- **S4 — Edge provenance tagging** (`EXTRACTED`/`INFERRED`-style, Graphify
  exemplar): cie already scores per-node confidence + records verdicts;
  extending provenance to call-graph edges would make a claimed
  differentiator visible in query output. Effort M. Verify:
  `callers()` output distinguishes resolved-vs-name-matched call sites —
  directly addresses the published "3 of 6 unresolved" gap.

## WATCH (promotion triggers)

- **W1 — CodeGraph's "platform is coming": per-PR what-to-test/breakage**
  [vendor claim, pre-launch hosted beta]. Promotion trigger: the hosted
  beta or README ships PR-test-impact features → differentiator #1 is
  under direct attack from the category leader; re-scan immediately.
- **W2 — SocratiCode's MCP-policy-proxy sibling** [vendor claim]:
  standalone governance slice exists; promotion trigger: it ships a
  code-integration that makes client-type tool-hiding automatic.
- **W3 — code-graph-rag's runtime overlay** (test-run/eBPF → dynamic
  graph edges): promotion trigger: adoption signal (≥5k stars) or a cie
  user asking for it; would be its own project, not a patch.
- **W4 — cross-repo graphs** (gortex default): real architecture change;
  only promote on a real multi-repo user.

## Registry-change proposals

None auto-registered. Watchlist added for next scan: `justrach/codedb`,
`harshkedia177/axon`, `aovestdipaperino/tokensave`, `jpicklyk/task-orchestrator`.