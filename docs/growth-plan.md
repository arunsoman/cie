# Growth plan

**Last researched:** 2026-08-28, alongside `docs/competitive-landscape.md`
— read that first for the full competitor comparison this plan assumes.

## TL;DR

CodeGraph's 47k-in-5-months growth wasn't bought with marketing spend or a
viral launch thread — there wasn't one. It came from removing every point
of friction between "developer hears about it" and "developer sees a
number go down," then riding GitHub's own trending flywheel once enough
early users crossed that line. cie cannot copy that playbook today — it
has real prerequisites CodeGraph didn't (no MCP protocol support, a
Neo4j setup requirement, zero published benchmarks). This plan is
ordered: **Phase 0 closes those gaps before any distribution effort is
worth spending**, because distribution into a broken first-run experience
just wastes the attention it earns.

## Case study: how CodeGraph actually grew

Researched from public sources (see `competitive-landscape.md`'s source
list, plus [Ry Walker's CodeGraph deep-dive](https://rywalker.com/research/codegraph)) —
this is what's documented, not speculation:

| Factor | What CodeGraph did |
|---|---|
| **Zero-friction first run** | Single local SQLite file, no server, no signup, no API key. Nothing to configure before the first result. |
| **Meets users where they already are** | Works with 8+ agent clients out of the box (Claude Code, Codex, Gemini, Cursor, OpenCode, AntiGravity, Kiro, Copilot) — relevant to almost any developer regardless of which tool they already use. |
| **One sharp pitch, one pain point** | "Understand any codebase as a graph" — aimed squarely at "the agent re-discovers my codebase from scratch every session," a pain every agentic-coding user already feels. Not a feature list. |
| **Quantified, honestly-caveated benchmarks** | Published median 58% fewer tool calls / 47% fewer tokens / 22% faster, across 7 real repos — *and* disclosed the range ("25–40% on small codebases, near break-even on response-heavy ones") instead of only the best number. An independent reviewer's own benchmark (70% reduction) beat the vendor's claim — third-party verification exceeding the vendor's own number is a rare, high-trust signal. |
| **Launched into a wave already cresting** | Went out during 2026's steep rise in agentic-coding-tool usage — the pain point was already top-of-mind for the exact audience it needed. |
| **The trending flywheel** | No documented Hacker News launch thread. Growth is tied to hitting #1 on GitHub Trending (May 16) and staying on Trendshift's daily/weekly/monthly lists from May 20 — trending visibility compounds on itself once a project crosses whatever threshold gets it there. |
| **A monetization path behind the free tier** | A hosted-platform waitlist (`getcodegraph.com`) sits behind the free local tool — the OSS release functions as a lead-generation funnel, which is a plausible reason real (if undisclosed) effort went into visibility despite no public marketing trail. |

**Worth being honest about, not just copying the wins:** engagement is
shallow relative to star count (115 watchers / 2,893 forks / 224 issues
against 47k+ stars) — a mismatch the research itself flags as atypical,
though it found no evidence of botting. ~91% of commits are from one
person — a real bus-factor risk the growth curve doesn't fix. Read: stars
are a discovery/trust signal, not proof of a healthy, sticky user base.
Growth and retention are different problems; this plan addresses growth,
and Phase 2 below is specifically about not repeating CodeGraph's
retention gap.

## Gap analysis: what's true for CodeGraph at launch that isn't true for cie today

| | CodeGraph at launch | cie today |
|---|---|---|
| First-run setup | Single file, zero config | Requires a running Neo4j instance |
| Agent-client compatibility | 8+ clients via MCP | **None** — no MCP protocol support |
| Published benchmarks | Yes, 7 repos, honest variance | None run |
| Pitch | One sentence, one pain point | Five capabilities (graph + tasks + quality + policy + fs) — accurate, but not a hook |
| Directory listings (mcpservers.org etc.) | N/A at launch, added as it grew | Not listed anywhere; the directory alone now lists 11,469 servers — listing is table-stakes, not a differentiator on its own |

Pushing distribution effort at cie in its current state means every new
visitor hits "clone this, then go set up Neo4j, then there's no way to
actually plug it into the agent I use" — the CodeGraph playbook depends
on the opposite of all three. Phase 0 exists to fix that before Phase 1
spends any attention on it.

## Phase 0 — status: done (2026-08-28)

All four items shipped and verified end-to-end, not just unit-tested:

1. **MCP adapter** (`cie/mcp_server.py`, `cie-mcp`) — real official `mcp`
   SDK, per-agent-type policy enforced by never registering a denied
   tool. Caught and fixed a real bug while wiring it up:
   `write_files_atomic` had never been added to `WRITE_TOOLS` — a
   read-only policy could have called it. Verified with a real stdio
   JSON-RPC handshake from a completely fresh `pip install "cie[mcp] @
   git+https://github.com/arunsoman/cie.git"` — `tools/list` returned 84
   tools under `--policy inspector`, correctly excluding every write
   tool.
2. **Zero-config embedded backend** (`cie/embedded_repository.py`,
   `cie index`, `cie-mcp --embedded`) — one local SQLite file, full
   `Repository` protocol via a wrapped, already-verified in-memory
   implementation (adapted from protobox's own test fixture). Indexed a
   real 12K-line codebase (36 files) in 0.87s using the same two-pass
   loader (`cie load`'s own contract) — structural extraction plus real
   call/inheritance/test-edge resolution, not a simplified version.
   Task/QA tracking explicitly unavailable in this mode, fails fast with
   a clear message rather than silently degrading.
3. **Real benchmarks** (`docs/benchmarks.md`) — 3 tasks against that same
   real codebase, published honestly per this plan's own commitment: one
   tie, one clear win (`callers()` resolves correctly where grep is
   ambiguous), one real loss (`file_skeleton`'s response was larger than
   the raw file for one target) reported as found.
4. **Pitch + README** — rewritten to lead with a one-line hook and a
   genuinely-correct zero-config quickstart (verified against the actual
   commands, including the real MCP handshake above) instead of assuming
   Neo4j.

Phase 1 (launch mechanics) and Phase 2 (retention story) below are not
started — Phase 0 was scoped as the prerequisite, not the launch itself.

## Phase 0 — original prerequisites (do before any launch push)

1. **Ship an MCP protocol adapter over the existing tool surface.**
   `cie/tool_schema.py` and `cie/tool_policy.py` already produce
   Anthropic-style tool definitions and per-agent authorization — the gap
   is the wire protocol, not the underlying capability (this was already
   flagged in `competitive-landscape.md`). Without this, cie cannot be
   "one config line" away from trying in Claude Code/Cursor/Codex the way
   every competitor is.
2. **Add a zero-config embedded graph backend option.** CodeGraphContext
   already proves the pattern (Neo4j *or* FalkorDB Lite *or* KuzuDB,
   pluggable) — cie's own `Repository` protocol
   (`cie/repository.py`/`cie/neo4j_repository.py`) is already an
   abstraction over the graph store; a lightweight embedded default
   (SQLite- or KuzuDB-backed) that needs zero setup, with Neo4j staying
   available for real multi-project/team scale, closes the single
   biggest friction gap without giving up cie's actual graph-query power
   for users who do need it.
3. **Run real benchmarks, publish them honestly.** Before/after tool-call
   and token counts on a handful of real, public repos — the same shape
   CodeGraph published, including the honest range, not just the best
   number. Unverified claims are worse than no claims in a category where
   the research above already flags stars-vs-substance skepticism as a
   known pattern; a credible, caveated number is what actually moved
   trust for CodeGraph (the independent-reviewer beat was the strongest
   signal in the whole story).
4. **Write the one-sentence pitch, then let the rest follow.** cie's real
   hook — proven this session, not aspirational — is "the only code graph
   that extends to a language with no LSP and no tree-sitter grammar"
   (the nirdosha proof case) or "the only code graph that also tracks
   which tasks and tests actually implement which code" (the traceability
   layer). Pick one for the README's first line; keep the full five-part
   feature set as what a reader finds after that hook lands, not before
   it.

## Phase 1 — launch mechanics (once Phase 0 is real, not before)

1. **Directory listings** — `mcpservers.org/submit`, `awesome-mcp-servers`,
   `mcp.so`. Free, low-effort, and every competitor surveyed is listed
   there — it's how they were findable enough to end up in this
   research in the first place. Table-stakes, not a growth driver on its
   own given 11,469 existing listings — do it, don't expect it alone to
   move the needle.
2. **The Show HN CodeGraph skipped.** The research is explicit: no
   documented Hacker News thread exists for CodeGraph despite its
   velocity — that's an audience it left on the table, not a channel that
   doesn't work for this category (Serena, Aider, and others do have HN
   history). A real "Show HN: cie — a code graph that extends to any
   language" post, timed to Phase 0 completion with a working demo link,
   is a genuine opportunity to reach the audience CodeGraph's own
   channel gap missed.
3. **Communities where this audience already is.** r/ClaudeCode,
   r/LocalLLaMA, the Claude Code and Cursor Discord servers — post the
   benchmark numbers, not a feature list, matching what actually landed
   for CodeGraph (scattered r/ClaudeCode testimonials are cited as real
   engagement in the research even without organized marketing).
4. **Coordinate the launch day.** GitHub's trending algorithm rewards
   star velocity in a short window — spreading directory listings, the HN
   post, and community posts across the SAME day (Phase 0 complete, demo
   live, benchmarks published) maximizes the chance of crossing whatever
   threshold tips a project onto the trending page, which is what
   actually compounded CodeGraph's growth after the fact.

## Phase 2 — sustain differentiation, don't repeat the retention gap

CodeGraph's own weak spot is engagement depth relative to star count —
plausibly because "understand any codebase as a graph" is a one-time
"wow" a user doesn't need to come back for once they've tried it. cie's
task/QA traceability and continuous quality-governance layer
(`competitive-landscape.md`'s differentiators 2 and 3) are the retention
story CodeGraph doesn't have — those are things a team keeps querying
every day, not a one-time index-and-forget. The README/pitch sequencing
matters here: lead with the sharp one-line hook to win the first star,
but the docs a returning user actually reads should surface traceability
and quality reports quickly, since that's the reason to keep it installed
past week one.

## Open question this plan deliberately doesn't answer

CodeGraph's free tier is a funnel for a hosted commercial product
(`getcodegraph.com`). Whether cie should follow that model — pure OSS
adoption vs. an eventual hosted/commercial layer — changes what "growth"
optimizes for and is a real business decision, not a technical one. This
plan is written to work either way (Phase 0/1 are valid regardless), but
Phase 2's investment level should be revisited once that's decided.

## Sources

Same as `competitive-landscape.md`, plus:
[Ry Walker — CodeGraph](https://rywalker.com/research/codegraph),
[Awesome MCP Servers directory](https://mcpservers.org/),
[awesome-mcp-servers (GitHub)](https://github.com/wong2/awesome-mcp-servers).
