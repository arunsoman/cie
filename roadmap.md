# roadmap.md — the first 20 items, by priority

> Created 2026-08-30, from: `docs/competitive-delta-2026-08-30.md` (S/W
> items), `docs/competitive-landscape.md` (honest-gaps section), the
> goal.md log's named-not-hidden leftovers, and this session's own
> findings. Companion to `goal.md` (launch-readiness workstreams) — this
> file sequences the *product* work; goal.md keeps the process state.

**Baseline when this was written:** suite 171/171, live-surface
conformance 126 tools — 85 verified / 19 graceful / 18
unavailable-by-design / 4 backend-gated, **0 crashes**; HTTP surface now
enforces `ToolPolicy` (read-only by default, server-side); competitive
delta verified as of `snapshot-2026-08-30.json` with no differentiator
killed.

**Ordering constraints (non-negotiable, inherited):**
1. **The C1 gate** — Phase 1 launch mechanics (listings, posts) do not
   run before 0.1.0 stable exists. That's goal.md's own rule; P2 below
   is blocked on **R6**, and R6 is blocked on R1–R5 honestly closing.
2. **`competitive-landscape.md`'s own call**: out-of-box language breadth
   is the largest *table* gap, but maturity is the highest-*leverage*
   item — everything else is harder to trust while the alpha is
   single-author and days old. Hence stable-cut before language-count.
3. **Honesty rules carry over**: every item's definition of done is
   *verified against the real environment*, not written. Claims sourced
   from competitor READMEs stay labeled vendor-claims.

Legend: `[ ]` open · `[~]` in progress · `[x]` done (verified) ·
effort S/M/L is a guess, treated as a guess.

---

## P0 — before 0.1.0 stable can honestly cut

- [x] **R1 · Embedded MCP write-side parity** (M). The default surface
      `cie-mcp --embedded` exposed only `list_pending_tasks` write-side-
      adjacent — the task/QA layer, the README's headline differentiator,
      is read-only on the default install because `push_tasks`,
      `set_task_status`, `link_artifact`, `append_repair_events`,
      `record_coverage*`, `push_hierarchy` are HTTP *alias handlers*,
      not `ToolService` methods, so the MCP server (which introspects
      only `ToolService`) never sees them. Promote them into real
      `ToolService` methods.
      **Verify:** `tool-test-lab/dogfood_mcp.py` sees and executes the
      full task/QA surface against `.cie/tasks.db`; the
      test_tool_surface_invariants relationships updated, not broken.
      *Done 2026-08-30 (minus `push_hierarchy`, which needs its embedded
      backend — moved to R14).* Six tools promoted; WRITE_TOOLS gained
      them in the same commit (the shadowing trap is pinned by a new
      invariants test); `HTTP_WRITE_ALIASES` down to `push_hierarchy`
      only; surface 126 → 132; live MCP probe executed all six against
      `.cie/tasks.db` (push/status get/link/coverage/snapshot all ok);
      conformance 88/22/18/4, 0 crashes, fresh artifact committed;
      suite 205/205.
- [x] **R2 · CLI↔SQLite parity for the quickstart** (M). Query commands
      (`cie files` et al.) answered Neo4j-only while `cie index` wrote
      SQLite — the zero-config quickstart literally retried
      localhost:7687 four times before failing (session-7 log, named
      not fixed). Point the query layer at the embedded repo.
      **Verify:** fresh venv: `pip install -e .` → `cie index .` → every
      documented query command answers against `.cie/graph.db`.
      *Done 2026-08-30:* `--backend`/`--db`/`CIE_BACKEND`/`CIE_DB`
      selection seam in cie/cli.py (auto = embedded when a local
      graph.db exists); embedded task store wired into `tasks:*`;
      hierarchy says honestly Neo4j-only (R14); load/watch/bootstrap
      carry the embedded-equivalent hint; explicit-embedded on a missing
      db fails fast with a not_found envelope. Verified: fresh venv →
      psf/requests index → all query commands answer from .cie/graph.db,
      `callers("close")` = the benchmark's 3 resolved. Suite 184/184.
- [x] **R3 · Stale-surface sync, repo-wide** (S). README badge said
      "tests-155 passing" (suite is now 213); README and
      `docs/tool-selection-accuracy.md` said "81-tool surface" where
      labels now distinguish dated snapshots from live counts.
      **Verify:** `grep` sweep for "81-tool|tests-155|~121"; every cited
      number matches `pytest -q` and
      `tool-test-lab/surface_results.json` on the same commit.
      *Done 2026-08-30 (post-R1/R2/R5, the final counts pass):* badge
      155→213 (~z measured at this commit); 81-tool citations converted
      to dated-snapshot labels (08-30 run, 83 read-only today); README
      gains the durable tool-count-label convention block
      (132 ToolService / 83 inspector, introspection-derived); competitive-
      landscape + language-agnostic-design counts updated; CLI command
      count corrected 49→47 (verified by click tree walk). Dated
      CHANGELOG/competitive-delta records stay frozen per convention.
- [x] **R4 · Reconstruct the provenance of this repo's own plan docs** (S).
      `goal.md` derives from `docs/growth-plan.md` Phase 0.5 — that file
      doesn't exist (only goal.md references it). Either restore it from
      history/sessions or fold its live content into `roadmap.md` +
      `goal.md` and repoint every dangling link.
      **Verify:** `grep -rn "growth-plan"` returns only text that says
      it was folded/restored, with links resolving.
      *Done 2026-08-30:* folded (not restored — `git log --all` has no
      copy); goal.md gains a Provenance section, all 11 references now
      carry folded language, tombstone `docs/growth-plan.md` added for
      old deep links. Verify re-run clean.
- [x] **R5 · Shrink the unavailable-by-design surface** (M). 18 of 132
      tools 503 (`unavailable`) in a standalone install from zombie
      `core.llm` protobox imports (session-7/8 conformance). Decouple
      them or move genuinely host-only tools behind an explicit plugin
      surface — either way, every remaining 503 carries a machine-
      readable reason string.
      **Verify:** `tool-test-lab/surface_conformance.py` re-run:
      unavailable bucket 18 → ≤6, each with its reason asserted in the
      harness; README's "~132 tools" gains the honest footnote.
      *Done 2026-08-30 (decouple option).* Module-level `core.llm`
      imports deferred to the LLM call sites in 4 modules → 13 tools now
      run standalone; the remaining 5 (`qa`, `contracts_run`,
      `state_machine_run`, `community_summarize_run`, `decompose_page`)
      503 with `error.reason` slugs, pinned by a hand-curated registry
      test + a bucket-size gate; envelope `reason` field added;
      failing_context("") crash found & fixed en route. Conformance:
      100 verified / 23 graceful / 5 unavailable / 4 backend-gated / 0
      crashes. Suite 212/212.
- [ ] **R6 · Cut 0.1.0 stable — the C1 gate** (S once R1–R5 close).
      Release checklist: final suite + conformance green, R3 counts
      accurate, CHANGELOG 0.1.0 section dated, version bump, GH release
      notes that keep the alpha-era caveats, PyPI artifact, then close
      goal.md's C1. **This is a trust claim and the user's decision to
      make, not a version-string edit** (goal.md says so explicitly).
      **Verify:** clean-venv `pip install cie[mcp]` quickstart works
      against a third-party repo; tag + release live; C1 checkbox
      closed in goal.md by hand.

## P1 — differentiator defense + credibility (after stable)

- [x] **R7 · Edge provenance tagging** (L). `callers()`/`callees()`
      distinguish verified graph resolution from heuristic/name matches
      (Graphify's `EXTRACTED|INFERRED` is the exemplar; cie already has
      node-level confidence + `record_verdict`). This also makes the
      published "3 of 6 call sites resolved on requests" gap *visible in
      the tool's own output* instead of only in a benchmark doc.
      **Verify:** `test_graph_semantics_ground_truth.py`-style fixture
      asserts labels against known-by-inspection truth, both backends.
      *Done 2026-08-30.* Per-row `provenance` (graph vs
      heuristic-name-match, including on fallback legs) + envelope
      `resolution` from `CallResolutionStat` tallies persisted by both
      loaders in the edge pass (`callgraph.resolve_call_edges(stats_out=)`
      / `resolution_stats()`); `tests/test_edge_provenance.py` pins
      labels+counts against known-by-inspection truth (embedded) and the
      fallback leg (Neo4j-configured service, no server — the fallback
      path IS the other surface); live-verified on psf/requests —
      `resolution {19 total, 16 unresolved, 3 resolved}`, matching the
      published 3-resolved. Suite 239/239.
- [ ] **R8 · `cie export-html` — the shareable artifact** (M). Static,
      read-only, self-contained HTML of a project's graph centered on
      what nobody else can render: task→file→test chains
      (`traceability_chain`/`_orphans`), plus callers/file_skeleton
      views. No server, no auth surface (the safe slice of a "viewer";
      Graphify's graph.html and GitNexus are exemplars). Screenshot
      artifact joins demo.svg via a script, not a one-off.
      **Verify:** export psf/requests, open via `file://` with zero
      deps, capture through the same record-then-commit pattern.
- [ ] **R9 · Benchmark harness + third independent repo** (M). Turn
      docs/benchmarks-requests.md's methodology into
      `scripts/benchmark.sh` (reproducible like `record_demo.sh` is the
      source of truth for the demo), run a third independent public
      repo, keep the honest-loss section. Add a token-per-query metric —
      codebase-memory-mcp's "120× fewer tokens" is a vendor claim; ours
      gets measured.
      **Verify:** the doc's numbers regenerate from the script; a
      fresh-clone reader can reproduce them end-to-end.
- [ ] **R10 · GraphRAG/embedding first-party benchmark** (M). The
      semantic layer (`cie/embed.py`, `cie/graphrag.py`) is the one
      capability with zero measurements ("Where competitors are
      genuinely ahead" lists it). Same-corpus comparison against
      claude-context/grepai; publish wins and misses.
      **Verify:** numbers land in `docs/competitor-benchmarks.md` with
      the same tool-version-table rigor as the 08-28 run.
- [x] **R11 · Streamable-HTTP transport for `cie-mcp`** (M). The mcp
      SDK's HTTP transport so browser/wire MCP clients connect without
      building a bespoke web client; `ToolPolicy` filtering flows
      through unchanged (inspector policy hides writes at the schema).
      **Verify:** a real browser-based MCP client (Inspector) connects
      over HTTP and sees exactly the schema set inspector policy
      predicts; write attempts are server-side-refused.
      *Done 2026-08-30.* Implementation turned out further along than
      the roadmap implied (the transport choice already parsed) — the
      real work was `--host/--port` wiring + verification: live harness
      (`tool-test-lab/dogfood_mcp_http.py`, official streamable-http
      client vs a spawned server) proves HTTP tools/list == exactly the
      85-tool inspector prediction and write attempts refused
      server-side; wiring unit tests pin stdio-vs-HTTP kwargs; loopback
      default + security doc reference; README quickstart block.
      (Browser-mode Inspector remains the manual human step — recorded
      as the harness's real-transport twin.)
- [ ] **R12 · Language #7: tree-sitter C** (M). The deferred D1-C item
      with a real reason: C's function name/params sit inside nested
      `function_declarator`s, so field-based helpers silently skip
      functions. Build the declarator-unwrapping logic for real.
      **Verify:** test_extract_go_rust-style suite against a real C
      parse; the naive-skip bug class asserted absent (a test that would
      fail if extraction silently returned nothing).
- [ ] **R13 · Languages #8–9: C++ and C#** (M). Share the R12
      declarator path; inheritance/impl resolution differs per language.
      Narrow but does not close the 6-vs-21-40+ gap — say so in the
      landscape doc, don't round up.
      **Verify:** same bar as R12, per language; README/landscape/
      pyproject updated in the same pass (the no-stale-docs rule).
- [x] **R14 · PRD-hierarchy port to embedded SQLite** (M). The last
      Neo4j-only feature (`cie.hierarchy`) — offered and not selected in
      session 3, still true. Completes "everything works on the default
      backend" story started by B1.
      **Verify:** hierarchy CRUD + lineage tests at the B1 depth
      (17-test bar) against SQLite; docs drop the "Neo4j-only" caveat.
      *Done 2026-08-30.* `SQLiteHierarchyRepository` (same protocol)
      over `.cie/hierarchy.db`; the three hierarchy tools promoted to
      real ToolService methods (the last HTTP-only alias handlers —
      `HTTP_WRITE_ALIASES` now EMPTY, pinned); `push_hierarchy` joined
      WRITE_TOOLS; `--no-hierarchy` / `hierarchy_tracking=False` opt-out
      with honest `unavailable[HIERARCHY_STORE_NOT_CONFIGURED]`; CLI
      `hierarchy:*` works embedded; stale docstring pointer corrected.
      21 tests + roundtrip+pins; surface 132 → 135, conformance re-run
      100/26/5/4, 0 crashes; suite 235/235.

## P2 — adoption & launch mechanics (gated on R6)

- [ ] **R15 · `cie init` one-command onboarding** (M). Detect installed
      MCP clients (Claude Code/Cursor/Codex), register the stdio server,
      write context files (GitNexus's AGENTS.md/CLAUDE.md and CodeGraph's
      installer are the exemplars; delta S2).
      **Verify:** fresh clone → `cie init` → client lists cie tools with
      zero manual config; captured for the demo cast.
- [ ] **R16 · `run`-tool isolation story made explicit** (M). `run` is
      already policy-gated (WRITE_TOOLS → 403 under the default read-only
      HTTP surface, this session) but the jail is cwd+timeout, "no
      container isolation yet" per routes.py. Ship docs + optional
      container mode; state the boundary plainly.
      **Verify:** doc states the threat model; a policy test pins that
      every networked surface refuses `run` by default.
- [ ] **R17 · Conformance in CI** (S). Run
      `tool-test-lab/surface_conformance.py` on every push: fail on any
      crash or silent surface-count change, so "126 tools, 0 crashes"
      stays a CI-verified invariant instead of a doc claim.
      **Verify:** intentionally break one tool → CI red; restore →
      green; the invariant is machine-checked from now on.
- [ ] **R18 · Directory listings** (S, human action). mcpservers.org
      submit + awesome-mcp-servers PR (goal.md E3) — external action
      under this account, so explicit go-ahead first, per the doc.
      **Verify:** entries live; README links them.
- [ ] **R19 · Repo trust signals + second-maintainer activation** (S).
      GitHub description/topics, release page rendered from CHANGELOG,
      2–3 `good-first-issue` tasks chosen against CONTRIBUTING's
      second-maintainer path (C4).
      **Verify:** a newcomer can reach a labeled starter issue from the
      README in ≤2 clicks; the contribution path in C4 reads as true.
- [ ] **R20 · Launch post, drafted but gated** (S + timing). Show HN /
      r/LocalLLaMA draft built from R8/R9 artifacts, every claim audited
      line-by-line against measured results. Publish only after R6 and
      the honesty stance holds (session-3 critique #10 declined —
      losses stay in the post too).
      **Verify:** a claim-by-claim table in the draft doc, each row
      footnoted to its measured source; nothing published from vibes.

---

## Not planned (watch-list, re-check at each competitive-delta scan)

- **W1** — CodeGraph's per-PR what-to-test platform announcement:
  re-scan the moment the hosted beta ships anything real (closest attack
  on differentiator #1).
- **W2** — SocratiCode's MCP-policy-proxy sibling: trigger if a code
  integration turns it into automatic client-type tool-hiding.
- **W3** — runtime call-graph overlay (code-graph-rag's test-run/eBPF
  trace): its own project, not a patch; promote on real user demand.
- **W4** — cross-repo graphs (gortex): real architecture change; promote
  only on a real multi-repo user.
- **F8/F9 + extension-host failure isolation** — pi harness environment,
  not cie the product; tracked in goal.md workstream F and
  `tool-test-lab/TOOL_TEST_REPORT.md`.

## Re-sequencing rule

When a competitive-delta scan fires (`/skill:competitive-delta`), any
`⚠ DIFFERENTIATOR IMPACT` finding promotes its WATCH item straight into
P1 ahead of whatever it outranks — the delta report, not this document,
is the source of truth the week the ground moves.