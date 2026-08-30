# roadmap.md — 45 items, by priority (P0: R1–R6, P1: R7–R14, P2: R15–R20, P3: R21–R45)

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
- [x] **R8 · `cie export-html` — the shareable artifact** (M). Static,
      read-only, self-contained HTML of a project's graph centered on
      what nobody else can render: task→file→test chains
      (`traceability_chain`/`_orphans`), plus callers/file_skeleton
      views. No server, no auth surface (the safe slice of a "viewer";
      Graphify's graph.html and GitNexus are exemplars). Screenshot
      artifact joins demo.svg via a script, not a one-off.
      **Verify:** export psf/requests, open via `file://` with zero
      deps, capture through the same record-then-commit pattern.
      *Done 2026-08-30.* `cie/export_html.py` + CLI `export-html`;
      data from read-only envelopes + the same repo reads the
      traceability tools use; zero-external-refs asserted in tests
      (plus XSS-escape pins); live: psf/requests export = 37 files / 34
      chains / 680 orphans / 215KB single file / **0 external
      references**; screenshots captured straight from `file://` via
      `scripts/record_export_html.sh` (committed: docs/images/
      export-{snapshot,chains}.png). Suite 241 → 246.
- [x] **R9 · Benchmark harness + third independent repo** (M). Turn
      docs/benchmarks-requests.md's methodology into
      `scripts/benchmark.sh` (reproducible like `record_demo.sh` is the
      source of truth for the demo), run a third independent public
      repo, keep the honest-loss section. Add a token-per-query metric —
      codebase-memory-mcp's "120× fewer tokens" is a vendor claim; ours
      gets measured.
      **Verify:** the doc's numbers regenerate from the script; a
      fresh-clone reader can reproduce them end-to-end.
      *Done 2026-08-30.* `scripts/benchmark.sh` + `benchmark_tasks.py`
      (3 shapes × both sides, chars-based token metric, labeled proxy);
      third dataset: urllib3 @ 85a8a9cf — wins and misses published
      (12 resolved caller edges vs 28 raw grep matches; 28/40
      unresolved; 2.24× skeleton compression) in
      `docs/benchmarks-urllib3.md`; requests numbers regenerate and
      match the doc (verified twice).
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
- [x] **R12 · Language #7: tree-sitter C** (M). The deferred D1-C item
      with a real reason: C's function name/params sit inside nested
      `function_declarator`s, so field-based helpers silently skip
      functions. Build the declarator-unwrapping logic for real.
      **Verify:** test_extract_go_rust-style suite against a real C
      parse; the naive-skip bug class asserted absent (a test that would
      fail if extraction silently returned nothing).
      *Done 2026-08-30.* `_c_declarator_name` (innermost identifier
      outside parameter_list — the mirror bug pinned too), params/return
      type through the declarator chain; `.c`+`.h` loaders; call sites
      resolve EXTRACTED; 9 tests incl. the never-silently-skips guard
      and name≠param-name; pyproject `tree-sitter-c>=0.23.0`; language
      counts 6→7 everywhere (competitive-landscape, README, pyproject).
      Documented v1 scope: structs are NOT class nodes; header
      prototypes stay out (the definition carries graph identity —
      duplicating them would fork same-name resolution, pinned by a
      test). Suite 254/254.
- [x] **R13 · Languages #8–9: C++ and C#** (M). Share the R12
      declarator path; inheritance/impl resolution differs per language.
      Narrow but does not close the 6-vs-21-40+ gap — say so in the
      landscape doc, don't round up.
      **Verify:** same bar as R12, per language; README/landscape/
      pyproject updated in the same pass (the no-stale-docs rule).
      *Done 2026-08-30 (counts now 9-vs-21-40+, stated as such).*
      C++: `.cpp/.cc/.cxx/.hpp/.hh` loaders; named class/struct bodies
      are CLASS + base_class_clause bases (access stripped, all
      `extends`); member methods via field_identifier in class bodies;
      out-of-class definitions via qualified_identifier's name part;
      inline-only emit (no prototype fork, matching C's decision).
      C#: `.cs`; class/interface/struct declarations; base_list first=
      extends/rest=implements (the documented C# convention);
      invocation_expression + member_accessExpression calls.
      Found+fixed live: `.h` stays C-scoped (a C++ namespace block
      mis-trees under the C grammar — caught by the R13 tests); the
      `cs`-vs-`csharp` language-key mismatch (caught by the interface
      test). 10 tests; suite 254 → 264.
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

- [x] **R15 · `cie init` one-command onboarding** (M). Detect installed
      MCP clients (Claude Code/Cursor/Codex), register the stdio server,
      write context files (GitNexus's AGENTS.md/CLAUDE.md and CodeGraph's
      installer are the exemplars; delta S2).
      **Verify:** fresh clone → `cie init` → client lists cie tools with
      zero manual config; captured for the demo cast.
      *Done 2026-08-30.* `cie/init.py` + CLI: auto-detect (Claude Code
      via project `.mcp.json`, Cursor via home config; Codex detected +
      snippet printed, never auto-edited), guarded JSON merges (existing
      entries byte-preserved, invalid JSON refused), managed context
      blocks (append/refresh-in-place), readonly-by-default (explicit
      `--policy full` opt-in). Verified: the registered entry handshakes
      over real stdio → 85 read tools, zero write tools visible
      (`scripts/record_init.sh` + `tool-test-lab/dogfood_mcp_stdio_list.py`);
      13 tests.
- [x] **R16 · `run`-tool isolation story made explicit** (M). `run` is
      already policy-gated (WRITE_TOOLS → 403 under the default read-only
      HTTP surface, this session) but the jail is cwd+timeout, "no
      container isolation yet" per routes.py. Ship docs + optional
      container mode; state the boundary plainly.
      **Verify:** doc states the threat model; a policy test pins that
      every networked surface refuses `run` by default.
      *Done 2026-08-30.* `docs/security.md` (threat model: jail IS / NOT
      IS / surface matrix); `CIE_RUN_WRAPPER` container seam
      (convenience-not-enforcement, byte-identical absent); enforcement
      pins: HTTP run-refusal monkeypatches Popen (403 must precede any
      spawn), wrapper-expansion test, MCP non-registration already
      pinned in test_mcp_server. Suite 264 → 266.
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

## P3 — the next 25 (R21–R45): defend, widen, mature (added 2026-08-31)

> Sequenced from `docs/competitive-delta-2026-08-30.md` (WATCH W1–W4 +
> the no-MUST verdict → differentiator *defense*) and
> `docs/competitive-landscape.md` (honest-gaps: language breadth,
> adoption/battle-testing, semantic maturity, maturity signals). The
> WATCH items are hereby PROMOTED into P3 per the re-sequencing rule —
> the owner directed the next tranche from these two docs on 2026-08-31,
> which the watch-list treated as the trigger. Promotions: W1 → R24/R25,
> W2 → R27, W3 → R26, W4 → R28. Executable plans live in `todo.md` P3.

### P3a — stable-story mechanics (small, unblocks adoption)

- [ ] **R21 · Ship `cie-mcp` to PyPI** (S). The 0.1.1 artifacts exist and
      pass twine + a clean-venv rehearsal; the upload itself needs the
      maintainer's PyPI token (username `__token__`) or, preferably, a
      `.github/workflows/publish.yml` on tag-push using a
      `PYPI_API_TOKEN` secret (trusted publishing needs the PyPI-side
      project entry first). **Verify (closes R6's step 5):** fresh venv,
      `pip install "cie-mcp[mcp]"` FROM PyPI, `cie index` + MCP handshake
      against the pinned requests clone — the exact rehearsal, against
      the real artifact. Feeds R18's listing links + R20's post.
- [ ] **R22 · CI hardening** (S). Merge the two green Dependabot PRs
      (click ≥8.5.0, tree-sitter-c ≥0.24.2 — both CI-passing), then add
      an OS matrix (ubuntu + macos; windows best-effort, allow-fail at
      first — tree-sitter/wheel availability varies). **Verify:** matrix
      green on a pushed commit; conformance gate unchanged (0 crashes).
      Basis: battle-testing gap (landscape).
- [ ] **R23 · Nightly benchmark CI** (S). A scheduled job running
      `scripts/benchmark.sh` (structural) against the pinned clones and
      committing a timestamped artifact of record; a threshold alert
      (e.g. callers-resolution or skeleton-ratio drift >20%) opens an
      issue instead of silently stale numbers. The semantic benchmark
      variant gates on a repo secret (`CIE_EMBED_*`) — runs only when
      present, skipped honestly otherwise. **Verify:** two consecutive
      nightly runs produce comparable artifacts; a deliberate drift test
      (once, on a branch) alerts.

### P3b — differentiator defense (WATCH promotions)

- [ ] **R24 · `cie impact` — PR test-impact** (M). *PROMOTED W1*
      (CodeGraph's per-PR what-to-test platform is the category leader's
      nearest attack on differentiator #1). Input: a diff/commit range
      (`sync_load_commit` + `sync_ast_delta` already exist) → output:
      blast radius (callers/affected_by chains from changed symbols),
      the RANKED test set derived from real TESTS edges + task links,
      and touched contracts/invariants — CI-ready JSON plus a human
      block. No competitor can derive a test set: they have no TESTS
      edges or task layer. **Verify:** ground-truth fixture (known diff
      → known test set, known-by-inspection), both backends; live
      psf/requests diff check matching the published resolution data.
- [ ] **R25 · PR review pack** (M). One artifact per change:
      `sync_load_commit`'s speculative-vs-canonical diff + R24's impact
      + drift/contract risks + orphan analysis, rendered in the R8
      export-html format (static, no server). Basis: W1 + differentiator
      #5 (two-graph git semantics — nobody surveyed has an equivalent).
      Depends on R24. **Verify:** pack for a pinned commit of a third-.
      party repo matches hand-derived expectations; script-generated.
- [ ] **R26 · Runtime evidence overlay** (M). *PROMOTED W3*
      (code-graph-rag's test-run/eBPF merges — the ecosystem's proof
      that users want observed behavior in a static graph; ours is
      test-run-level, not eBPF-level, and that's enough for v1). Ingest
      `pytest --junitxml` into the EXISTING `record_test_result`/
      TestExecution surface + measured coverage snapshots
      (`record_coverage_snapshot`), persist as OBSERVED edges; R24's
      ranking then uses observed failure history, and envelopes carry a
      staleness flag. **Verify:** replay a real junitxml into a fixture
      graph; `impact` output reflects the observed run in a
      ground-truth test.
- [ ] **R27 · Policy profiles 2.0** (M). *PROMOTED W2* (SocratiCode's
      MCP-policy-proxy sibling — keep server-side governance ahead):
      named policy FILES (allow/deny tool patterns + tool-group
      wildcards), `cie init --policy <file>` per-client binding,
      `--policy-file` on cie-mcp, and a bounded refusal-audit log
      (who-asked-what-was-denied, size-capped). **Verify:** profile
      tests per surface (stdio/HTTP), audit-log rotation test, the
      existing ToolPolicy invariant set extended not replaced.
- [ ] **R28 · Multi-repo workspaces** (L). *PROMOTED W4* (gortex's
      cross-repo default) — **still gated on the watch-list trigger:** a
      real multi-repo user must ask before this starts (re-sequencing
      rule). Registry of indexed roots (`cie workspace add/list`),
      project-qualified tool results, workspace-level impact/search.
      **Verify:** two-repo fixture returns correctly-qualified symbols
      with no cross-root id collisions.

### P3c — semantic layer completion (R10's follow-throughs)

- [ ] **R29 · First-party chat fallback → `qa` fully standalone** (M).
      Mirror R10's embed tier-3 for CHAT completions:
      `cie/llm_compat.py` implements the minimal Prompt/ask surface
      (`QaLlmOutput`-style parsing, system+user messages, JSON-mode
      output) over any OpenAI-compatible chat endpoint, gated exactly
      like the embed client (explicit DSN + key — no accidental
      network). `qa`, `rerank`, and the host-gated runners
      (`contracts_run`, `state_machine_run`, `community_summarize_run`)
      get a standalone path; the unavailable registry shrinks toward
      the pinned minimum (decompose_page remains plugin-gated).
      **Verify:** `tests/test_llm_compat.py` (mocked transport: request
      shape, tool-call-free JSON parsing, degrade paths) + live
      conformance showing the bucket shrink; `test_unavailable_reasons`
      updated at the same commit (the registry trap).
- [ ] **R30 · No-LLM rerank** (S/M). A structural re-scoring pass
      (lexical coverage + dense + graph degree + TESTS-linkage boost)
      used when no LLM is reachable, behind the existing
      `use_reranking` seam; A/B against R29's LLM rerank AND no-rerank
      on the 16-question labeled set — publish whichever wins per
      corpus, misses included. **Verify:** labeled-set MRR deltas in the
      benchmark doc; deterministic (seeded) behavior test.
- [ ] **R31 · Benchmark corpus #4 + tokenizer-pinned counts** (M).
      Fourth corpus = a real C repo exercising R12/R13 (candidates:
      libuv, sqlite amalgamation — pick on the ambiguous-name property);
      plus pin ONE tokenizer (add `tiktoken` as a benchmark-only extra,
      not a runtime dep) so the token-efficiency numbers stop being
      chars-heuristic: re-publish chars AND tokens for every task in
      every benchmark doc, dated re-run notes — the apples-to-apples
      ground for the "120× fewer tokens"-class vendor claims (labeled
      vendor-vs-measured, per the standing rule). Depends on R29 for
      the QA-task slice (or runs structural-only first).

### P3d — language breadth (the landscape's biggest table gap)

- [ ] **R32 · Language #10: Kotlin** (M). tree-sitter-kotlin grammar;
      field-based name extraction (C#-like, friendlier than C's
      declarators) — the known trap is receiver-function syntax (`fun
      String.foo()`) and companion objects; `test_extract_kotlin.py` at
      the R12 bar (exact-set positive assertions + a naive-skip guard);
      README/landscape/pyproject sweep in the same PR. **Verify:**
      real-parse fixture suite + counts sweep.
- [ ] **R33 · Languages #11–12: PHP + Ruby** (M). One pass, same bar:
      PHP `method_declaration` with `name` field (friendly) + `->`/
      `::` receiver call resolution; Ruby `def`/`call` nodes + `def
      self.x` receivers; Go/Rust-style documented-gap assertions per
      language. Both update the counts sweep same-PR. **Verify:** two
      test suites + landscape table row (9 → 12, honest phrasing kept:
      still far from 21–40+).
- [ ] **R34 · Close Go/Rust's documented gaps: import edges + docstrings**
      (M). Landscape §7 names them honestly: Go/Rust extract symbol
      structure but extract NO import edges and NO docstrings. Port the
      Python loaders' import-map path (`.h`-style file-hub resolution)
      and docstring attachment to both; `tests/test_extract_go_rust.py`
      grows the assertions; the "documented gap" comment flips to
      implemented. **Verify:** import-edge ground truth + docstring
      attach tests; no silent-skip regressions (the D1 guard stays
      green).

### P3e — graph richness & MCP surface (parity where competitors lead)

- [ ] **R35 · Non-code artifacts as nodes** (M). Dockerfile / compose /
      Makefile / CI yaml / requirements-pyproject files become FILE
      nodes with DESCRIBES edges (codebase-memory-mcp indexes
      Dockerfiles/K8s as graph nodes [vendor claim]) so impact and
      traceability cover build/deploy surfaces. **Verify:** fixture
      with a Dockerfile + a workflow yaml asserting node kinds/edges;
      export_html + impact include them.
- [ ] **R36 · Freshness contract** (M). `watch` mode currently launches
      the observer but staleness is only reportable
      (`freshness_report`): make watch auto-apply `sync_ast_delta` on
      debounce (the seam already exists; `start_watch` runs Observer
      now) and stamp every read envelope with `as_of`/`stale` — the
      zero-staleness guarantee CodeGraph's auto-sync advertises [vendor
      claim]. **Verify:** touch-edit-query loop test (watchdog real
      Observer in CI — skip-with-reason on flaky fs events), envelope
      stamp test.
- [ ] **R37 · MCP resources + prompts** (M). GitNexus parity: expose
      resources:// (project stats, chains, the export-html asset)
      and prompt templates (impact-report, traceability-audit,
      onboarding-summary) — read-only by construction, server-side
      filtered by policy like tools. **Verify:** resources/list +
      prompts/list over the stdio dogfood harness; policy refusal for
      non-permitted content.
- [ ] **R38 · `cie serve-ui` — local web viewer** (L). Localhost-only,
      read-only browser UI over the embedded graph (graph browser,
      chain view, search) — the server cousin of R8's static export;
      the safe slice: no write surface, no auth model, loopback bind +
      policy note. Basis: Graphify's UI artifact + CodeGraph's platform
      adjacency; complements export-html (interactive vs frozen).
      **Verify:** harness drives headless-Chrome over the UI; one
      committed screenshot via the record-script pattern; security.md
      gains the serve-ui threat model (loopback, read-only).
- [ ] **R39 · Multi-project server** (M). One `cie-mcp` serving N
      indexed projects: `--project` repeatable / `--projects-file`,
      tool `project` arg honored server-side, per-project DB isolation,
      toolset summary in `describe`. Distinct from R28 (analytics
      ACROSS repos) — this is serve-time routing BETWEEN them; R28
      builds on it. **Verify:** two-project stdio handshake shows both
      toolsets, no cross-project id leaks (collision test).

### P3f — credibility & productization (landscape maturity/credibility)

- [ ] **R40 · Structural benchmark corpus #4** (M). The R9 harness
      against the same C repo chosen in R31 (or a second Python repo if
      the C parse is still young): the two-column honesty format, 4th
      dataset published, landscape's "two/three datasets" sentences
      updated with dated re-runs. **Verify:** fresh-clone reproduction
      from the script alone (GFI-3's bar).
- [ ] **R41 · Token accounting, pinned** (S). Superseded by R31's
      tokenizer work in the semantic doc; this item carries the rest:
      re-publish chars-vs-tokens for every historical benchmark table
      with the pinned tokenizer, and one explicit comparison paragraph
      vs "120× fewer tokens"-class vendor claims (labeled, not
      implied-equal). **Verify:** docs regenerate from scripts with
      both metrics; no prose number floats without a table.
- [ ] **R42 · Scale milestone: ≥100k-LOC corpus** (M).django or a cpython
      subset — publish index time/memory, per-tool query latency at
      scale, and conformance-on-big-repo results; the response to
      SocratiCode's "2.45M-LOC" vendor framing is OUR measured scale,
      not a counter-claim. **Verify:** numbers + methodology in a dated
      doc; suite unaffected (fixture-scoped).
- [ ] **R43 · `cie doctor` + init breadth** (S). `cie doctor`:
      index freshness, orphan tasks/tests, stale-db/schema-version
      checks, env config echo, MCP self-handshake — one command that
      says what's wrong (trust-signal item); `cie init` gains Cline/
      Continue/Windsurf/VSCode-agent detection (config-presence
      discipline as R15). **Verify:** doctor on a healthy + a broken
      fixture (each finding asserted); one new client config merged
      idempotently.
- [ ] **R44 · Gate packs** (S/M). `cie gate --profile
      pr|nightly|refactor` composing the EXISTING checkers
      (traceability_coverage, invariant_violations, clone_detect_run,
      drift_detect_run, coverage_gaps, tech_debt_report) with exit codes
      + JSON + thresholds — the quality-governance differentiator made
      CI-consumable in one command (pairs with R22's CI + R24's impact).
      **Verify:** a fixture repo wired into CI that goes red on an
      intentionally-broken commit and green on restore (R17's proof
      pattern).
- [ ] **R45 · Last-503s: reference decompose plugin** (S). `decompose_p
      age` is the only HOST_PLUGIN-gated tool left once R29 lands: ship
      a REFERENCE detector plugin (html.parser-based interactive-element
      extraction) as the extension example + docs/plugin.md — the
      availability registry ends at zero forced-503s with env-gated
      optional backends honestly stated. **Verify:** registry test
      end-state asserted; plugin example tested.

**P3 dependency sketch:** R21 → (R22, R23) is the mechanical spine;
R24 → R25; R26 feeds R24's ranking (either order, note in PR); R29 →
R30/R31/R45; R39 → R28; R36/R37/R38/R43/R44 independent;
R40/R41/R42 independent; R32/R33/R34 independent. Re-sequencing rule
unchanged: a DIFFERENTIATOR IMPACT finding from the next delta scan
pre-empts everything in this tier.

## Not planned (watch-list — W1–W4 now PROMOTED into P3 above)

*(Promoted 2026-08-31 per the owner's next-tranche direction: W1 → R24/
R25, W2 → R27, W3 → R26, W4 → R28. This list keeps the triggers for the
remaining non-promoted items and the re-check cadence.)*

- **W1** — CodeGraph's per-PR what-to-test platform announcement:
  re-scan the moment the hosted beta ships anything real (closest attack
  on differentiator #1). → **PROMOTED into R24/R25 (P3).**
- **W2** — SocratiCode's MCP-policy-proxy sibling: trigger if a code
  integration turns it into automatic client-type tool-hiding.
  → **PROMOTED into R27 (P3).**
- **W3** — runtime call-graph overlay (code-graph-rag's test-run/eBPF
  trace): its own project, not a patch; promote on real user demand.
  → **Test-run level PROMOTED into R26 (P3); eBPF-level stays here.**
- **W4** — cross-repo graphs (gortex): real architecture change; promote
  only on a real multi-repo user.
  → **PROMOTED into R28 (P3), entry itself keeps the real-user gate.**
- **F8/F9 + extension-host failure isolation** — pi harness environment,
  not cie the product; tracked in goal.md workstream F and
  `tool-test-lab/TOOL_TEST_REPORT.md`.

## Re-sequencing rule

When a competitive-delta scan fires (`/skill:competitive-delta`), any
`⚠ DIFFERENTIATOR IMPACT` finding promotes its WATCH item straight into
P1 ahead of whatever it outranks — the delta report, not this document,
is the source of truth the week the ground moves.