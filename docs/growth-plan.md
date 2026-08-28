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

## Phase 0.5 — hook, framing, and trust gaps (status: planned, 2026-08-29)

Phase 0 closed the three structural gaps (MCP, zero-config, benchmarks).
It did not fix *how the project presents itself* once someone actually
lands on the README — a harsher self-read of the current state surfaced
10 specific problems, grouped below into 5 workstreams. Ordered by
leverage: the pitch/framing fixes are near-free and should ship first;
the trust and coverage fixes take longer and don't block a launch, but
do block a *good* launch.

### A. The pitch itself (critique #1, #2)

The opening line is still a capability list ("a code graph your AI coding
agent can actually use" → five bullets), and the very next doc a reader
is pointed to studies a 47k-star competitor cie explicitly can't catch
yet. Neither is a hook.

1. **Rewrite the README's first line to one sentence, one pain point** —
   Phase 0 item 4 already named the two real candidates and never acted
   on them. Pick one:
   - *"The only code graph that works on a language with no LSP and no
     tree-sitter grammar."* (proven — the nirdosha adapter)
   - *"The only code graph that also tracks which tasks and tests
     actually implement which code."* (proven — task/QA traceability)
   Move the current five-bullet pitch to *after* the hook, where
   `competitive-landscape.md` already puts the fuller comparison.
2. **Stop opening with the CodeGraph gap-analysis framing.** The
   47k-vs-0-stars table is real research and should stay, but not as the
   first thing a prospective user or contributor reads. Move
   `competitive-landscape.md`'s "honestly behind" table below the "where
   cie genuinely excels" section (currently the opposite order), and
   drop the direct 47k-stars comparison from the README's top-level
   pitch entirely — it belongs in the linked doc, not the headline.

### B. The retention story is behind a database (critique #3) — status: done (2026-08-29)

Was: task/QA traceability — the one differentiator no competitor has —
was Neo4j-only, and the two-command zero-config quickstart explicitly
couldn't show it (`NullTaskRepository` failed fast). Fixed:

3. **Shipped `cie.embedded_task_repository.EmbeddedTaskRepository`** —
   SQLite-backed, the full `TaskRepository` protocol (not a subset):
   push/list/status/QA-status, dependency traversal (the actual
   traceability query), artifacts, repair events, deletion, and all
   three consistency validators (cycles, coverage, API contracts).
   Reuses `cie.task_repository.plan_push` — the exact validation
   function `Neo4jTaskRepository.push_tasks` calls — so acceptance/
   rejection semantics can't quietly drift between the two backends.
   `build_tool_service_embedded` now constructs this by default (a
   second local file, `.cie/tasks.db`); `task_tracking=False` (or
   `cie-mcp --no-task-tracking`) still gets the old fail-fast
   `NullTaskRepository` for callers that want the smaller footprint.
   17 new tests (`tests/test_embedded_task_repository.py`), including
   the actual "which files implement task X, are they tested" query and
   a cycle-detection case that bypasses push-time prevention to test the
   validator itself, plus 3 tests updated/added in
   `tests/test_embedded_repository.py` for the new default and the
   `task_tracking=False` opt-out. Full suite (58 tests) verified green.
   **Deliberately not in scope**: `cie.hierarchy`'s separate PRD tree
   (Module/Feature/Workflow/UseCase/UserStory) and its three tools
   (`prd_coverage`/`prd_orphans`/`prd_traceability_chain`) are still
   Neo4j-only — they call `cie.factory.get_hierarchy_repo` directly,
   independent of which backend built the `ToolService`. That's a
   separate, larger piece of work, not silently folded into this one.

### C. Trust signals undercut the pitch (critique #4, #5, #6)

Alpha, one day old, solo author, 4 test files for a 28k-line/121-tool
surface — all true, all already disclosed in the repo's own docs, which
is good practice but doesn't make a due-diligence reader more willing to
bet on it. None of this is fixable by launch day; it's fixable by not
launching *before* it's less true.

5. **Don't coordinate a launch-day push (Phase 1) until there's a 0.1.0
   stable, not an alpha two days old.** Let the alpha sit through real
   dogfooding first — the a1→a2 fix (MCP SDK version mismatch caught by
   CI, not by a user) is exactly the kind of bug a launch-day crowd would
   hit publicly instead.
6. **Grow the test suite before growing the audience.** Correction on
   verification: `cie/test_orchestration.py` and `cie/test_synthesis.py`
   don't crash pytest today — `pyproject.toml`'s `testpaths` already
   scopes collection to `tests/` specifically to avoid it (confirmed by
   running `pytest --collect-only`: 38 tests, no error). The warning
   lives in a `pyproject.toml` comment, not the README as first assumed —
   worth fixing that comment's visibility (a stray `pytest` with no args
   run from outside the documented flow, e.g. by a new contributor, is
   still one `git mv`/rename away from breaking), but it is not an open
   bug. — **status: done (2026-08-29).** Three new test files added:
   `tests/test_embedded_task_repository.py` (17 tests, covers B's new
   task/QA layer), `tests/test_clone_detect.py` (7 tests, quality-
   governance — zero coverage before), `tests/test_lang_adapter.py` (11
   tests, the language-adapter registry — zero coverage before, despite
   being the mechanism the "extends to any language" claim rests on).
   Plus `tests/test_extract_go_rust.py` (9 tests) for workstream D below.
   Suite: 4 files/38 tests → 8 files/85 tests.
7. **Bus factor is a docs problem right now, not just a people problem.**
   `CONTRIBUTING.md` should say explicitly what's needed to become a
   second maintainer (which modules are safe entry points, what review
   bar applies) — cheap to write, and it's the honest next step given the
   growth-plan already flags this same risk in CodeGraph and would
   otherwise be pointing at itself without an answer. — **status: done
   (2026-08-29).** "Becoming a second maintainer" section added.

### D. Language ceiling (critique #7)

"Works with no tree-sitter grammar" is true but requires the user to
write a `LanguageAdapter`. Every competitor's larger out-of-box language
count wins the "index my repo right now" moment cie currently loses.

8. **Add 2-3 more tree-sitter grammars that are near-free** (tree-sitter
   already has grammars for Go, Rust, C/C++, Ruby — the extraction layer
   is already grammar-agnostic per-language, so this is adapter-writing,
   not architecture work). Target languages by what's actually common in
   the repos people will test against first, not exhaustive coverage. —
   **status: Go and Rust done (2026-08-29), C explicitly deferred.**
   Both added to `_LANG_LOADERS` with real, verified extraction: function/
   method declarations, signatures (return-type field names differ per
   grammar — Go's `result`, Rust's `return_type`, neither literally
   `"type"` the way Java's does — checked against real parses, not
   assumed), and receiver/impl-method call resolution (Go's
   `selector_expression`, Rust's `field_expression` — each uses yet
   another field-naming convention for the same "obj.member" shape;
   `_call_target` generalized rather than special-cased per language). A
   real bug caught before it shipped: the naive fix (just adding these
   node types to `_ATTRIBUTE_TYPES`) would have silently returned the
   *receiver* as the called name instead of the real method for both
   languages — `tests/test_extract_go_rust.py`'s call-site tests assert
   the receiver name is specifically absent from resolved call names, not
   just that the correct name is present. Two honest, documented gaps for
   Go/Rust specifically (not silently glossed): no import-edge extraction
   (`_collect_imports` dispatches by language name with no Go/Rust
   branch) and no docstring extraction (`//` comments aren't scanned).
   Receiver/impl methods surface as `kind=FUNC` not `METHOD` (neither
   language has a `_CLASS_TYPES` match — Go structs and Rust `impl`
   blocks aren't classes) — still fully searchable/callable/signature-
   correct, just a coarser classification than Java/Python/JS methods
   get. **C was evaluated and explicitly not attempted**: `tree-sitter-c`
   parses a function's name/params two levels deep inside a
   `function_declarator`, not exposed via a direct `name`/`parameters`
   field the way every other supported language is — `_declared_name`/
   `_params_text`'s existing field-based lookups would silently return
   `""`/`"()"` instead of a real name/params (an empty name means the
   function gets skipped entirely, not partially extracted — see
   `_emit_function`'s `if not name: return`) rather than working, and
   fixing that needs real bespoke declarator-unwrapping logic. That's
   meaningfully
   more work than Go/Rust's shared-mechanism fit, and shipping it
   unverified risked exactly the "looks wired up, silently extracts
   nothing" failure mode this whole plan exists to avoid — better
   scoped as its own follow-up than rushed here.
9. **Or, cheaper: ship one worked example of a from-scratch adapter** (a
   "add support for language X in an afternoon" doc/video) so the
   long-tail argument becomes something people can *see* done once,
   rather than a claim they have to take on faith before trying it
   themselves.

### E. Nothing to quote, nothing to share (critique #8, #9, #10)

`docs/benchmarks.md` exists and is honest, but it's not where a
skimming reader or a second-hand sharer will ever see it. There is no
image, video, or listing anywhere — the repo is currently un-shareable
by design, not just by omission.

10. **Put the one real number in the README, above the fold.** From
    `docs/benchmarks.md`: *"1 tool call instead of 3, and correct by
    construction, for ambiguous-name lookups a grep-only agent can't
    disambiguate"* is the one honestly-earned quotable line today — use
    it, with the same "here's where it didn't help" honesty already in
    the doc, not a cherry-picked headline number.
11. **One real artifact**: a terminal-cast or short GIF of the actual MCP
    handshake in Claude Code (`tools/list` returning the real tool count)
    — this is Phase 0's already-verified behavior, just never captured
    on screen. Cheapest possible distribution asset given the work is
    already done.
12. **Directory listings** (Phase 1 item 1) can move earlier — they're
    independent of the alpha/test-coverage concerns above and cost
    nothing to do now.
13. **Re-sequence `competitive-landscape.md`** per item 2 above — same
    fix, listed here again because it's specifically what makes the doc
    an adoption deterrent rather than a trust-builder: lead with section
    "Where cie genuinely excels," follow with the honest gaps, not the
    reverse.

### What this changes about Phase 1

Phase 1 (launch mechanics: directory listings, Show HN, community posts,
coordinated launch day) stays blocked — not on Phase 0's four items,
which are done, but on **workstream C1 specifically**: a maturity bar
past "alpha, days old." A/B/C2/C4/D have all landed (2026-08-29): the
hook isn't a feature list anymore, the retention story is triable in the
zero-config path, the test suite covers what it didn't, and language
breadth grew from 4 to 6. What's left gating Phase 1 is a real 0.1.0
stable release (C1) and, separately, E's launch-asset work (a GIF, the
directory listings themselves) — see E below, explicitly excluded from
this pass. Item 12/E3 (directory listings) is the one Phase 1 item safe
to do early regardless, since it doesn't depend on any of the above.

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
