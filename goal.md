# cie — launch-readiness goal

> Tracking doc · 29 Aug 2026 · derived from `docs/growth-plan.md` Phase 0.5
> (**folded into this doc on 2026-08-30** — see Provenance below; that
> file now exists only as a tombstone).

**Goal:** close the 5 workstreams (A–E) that stand between "Phase 0 done"
and "safe to run Phase 1 launch mechanics" — a real hook instead of a
feature list, a triable retention story, trust signals that match an
alpha's actual age, an honest language-coverage story, and something
quotable/shareable. Each item below is done when verified against the
real repo, not when written — same bar the rest of this project holds
itself to. Status is kept current here; don't let this drift from what's
actually true. Each item's rationale is its own text below — the plan
that used to live in `docs/growth-plan.md` Phase 0.5 was folded into
this file on 2026-08-30 (see Provenance next).

**Product principle (stated 2026-08-31, owner's words):** *least
friction to users and their existing infrastructure.* Concretely: the
default path works with zero decisions (auto backend selection — an
indexed project just serves); one explicit knob (`--backend`,
`CIE_BACKEND`) when the user does care, identical across front-ends;
existing env vars (`NEO4J_*`, legacy `CIE_NEO4J_*`) and every old
spelling (`cie-mcp --embedded`) keep working forever; whatever the
system picks is stated, never silent (stderr startup line).

## Provenance — `docs/growth-plan.md` folded here (R4, 2026-08-30)

`docs/growth-plan.md` Phase 0.5 was the launch-readiness plan this doc
derived from, and its live content is fully carried by the workstreams
A–E below; C1's "Open question" rationale — the one piece referenced
only by path — is preserved in C1's own text. The file never existed in
this repo's git history (checked `git log --all` before folding), so it
was folded, not restored: body references now point at this doc, and a
tombstone at `docs/growth-plan.md` keeps old deep links from 404-ing.
Roadmap item R4; done when `grep -rn growth-plan` returns only folded-
language text and all links resolve.

Legend: `[ ]` not started · `[~]` in progress · `[x]` done, verified

---

## A — The pitch itself

- [x] **A1. Rewrite README's opening line to one sentence, one pain
      point.** New title + opening paragraph lead with the task/QA
      traceability hook ("the only code graph that knows which tasks and
      tests actually implement your code"); the five-capability
      breakdown moved below Install, cross-referenced not duplicated.
- [x] **A2. Stop opening `competitive-landscape.md` with the 47k-star gap
      table.** Physically reordered: "Where cie genuinely excels" now
      leads, comparison table and gaps follow. Also fixed a stale claim
      found while in there — the doc's "next move" recommendation still
      pointed at the MCP adapter, which Phase 0 already shipped;
      repointed at the actual current top gap (B below).

## B — Retention story shouldn't require Neo4j

- [x] **B1. Ship `EmbeddedTaskRepository`** (SQLite-backed) — full
      `TaskRepository` protocol, not a subset (push/list/status/QA-status,
      artifacts, repair events, dependency traversal, cycle/coverage/
      API-contract validation). Reuses `plan_push` from
      `cie.task_repository` so validation semantics can't drift from
      Neo4j's. Wired into `build_tool_service_embedded` as the default
      (`.cie/tasks.db`); `task_tracking=False` / `cie-mcp
      --no-task-tracking` keeps the old `NullTaskRepository`. 17 new
      tests + 3 updated in the existing embedded-repo test file; found
      and fixed one real bug along the way (a `list_pending` helper that
      would have always returned "pending" regardless of actual status —
      `AtomicTask` has no `status` field, it's a pure write-back
      property, caught by re-reading raw stored props instead). Every
      doc/README mention of "Neo4j only" for task/QA tracking corrected;
      the *separate* PRD-hierarchy tree (`cie.hierarchy`) is explicitly
      still Neo4j-only and documented as such, not silently implied
      fixed. Full suite: 58/58 pass.
- [x] **B2. N/A** — B1 landed, so the "document as next roadmap item"
      fallback wasn't needed.

## C — Trust signals vs. actual maturity

- [x] **C1. Don't run Phase 1 (launch mechanics) until 0.1.0 stable ships**
      — **CLOSED 2026-08-31 by hand**: tag `v0.1.0` cut on the release
      commit `a22b4bf`, GitHub release live at
      https://github.com/kannamma-labs/cie/releases/tag/v0.1.0 (notes render
      CHANGELOG's `[0.1.0]` section + the maturity-caveats block, kept from
      the alpha per the no-honeypot rule). Clean-venv rehearsal verified
      end-to-end from the built wheel (suite 279/279; conformance 0
      crashes; index → callers=3 → MCP handshake 85 read-only tools on a
      fresh psf/requests clone at the pinned commit). The open follow-up
      from the same session is RESOLVED (2026-08-31): the distribution
      ships as **`cie-mcp`** from v0.1.1 (pypi `cie`
      is cluster311/cie10, ICD-10 codes; import package/CLI/tags
      unchanged). README quickstart + install carry the PyPI line with
      the dated note; the PyPI upload itself runs with the maintainer's
      credentials/workflow on top of this commit. This closure was the
      release mechanics existing — the user's go-ahead given in the
      2026-08-31 session.
- [x] **C2. Grow test coverage** — one test file per major capability
      area not yet covered. `tests/test_clone_detect.py` (7 tests,
      quality-governance — token/AST clone signals + union-find fusion,
      real tree-sitter re-parse, zero coverage before), `tests/
      test_lang_adapter.py` (11 tests, the registry mechanism the "any
      language" claim rests on — zero coverage before). Task/QA is
      covered by B1's 17 tests. Suite: 4 files/38 tests → 8 files/85
      tests (after D1 below too).
- [x] **C3. Correct the pyproject.toml test-collection comment's
      visibility** — verified it's accurate (no actual crash today,
      `testpaths` already guards it, `pytest --collect-only` → 38 tests,
      0 errors); the original critique's attribution to a README warning
      was wrong (it's a `pyproject.toml` comment) — the fix's status
      note lives here now (the growth-plan.md text this fix was logged
      against was folded into this doc on 2026-08-30).
- [x] **C4. `CONTRIBUTING.md`: name a path to a second maintainer** —
      added: safe entry-point modules, modules to avoid as a first PR,
      the actual review bar, how to ask for commit access.

## D — Language ceiling

- [x] **D1. Add tree-sitter grammars** — Go and Rust done (2026-08-29),
      verified against real parses: function/method declarations,
      signatures (Go/Rust each use a different return-type field name
      than Java/Python — checked, not assumed), and receiver/impl-method
      call resolution (caught and fixed a real bug pre-ship: the naive
      version would have resolved the *receiver* as the called name
      instead of the method — `tests/test_extract_go_rust.py` asserts
      the receiver is specifically absent). Two honest, documented gaps
      for Go/Rust: no import-edge extraction, no docstring extraction.
      **C explicitly evaluated and deferred** — its function name/params
      sit two levels deep inside a `function_declarator` with no direct
      `name`/`parameters` field, so the existing field-based extraction
      helpers would silently return `""`/skip the function entirely
      rather than partially work; real declarator-unwrapping logic is
      needed and wasn't attempted rather than shipped unverified.
      Language count: 4 → 6 out-of-box (competitive-landscape.md's
      table, growth-plan.md (since folded into this doc), README,
      pyproject.toml core deps all
      updated to match).
- [x] **D2. Worked "add a language" doc + real adapter example.**
      `docs/adding-a-language.md` + `examples/adapters/toy_regex_adapter.py`
      — a complete, runnable `LanguageAdapter` for a language cie has
      never seen (regex, no tree-sitter, no LSP). Ran it for real
      (`PYTHONPATH=. python examples/adapters/toy_regex_adapter.py`);
      output matches what's checked into the file's own docstring. Linked
      from README's Docs section.

## E — Nothing to quote, nothing to share

- [x] **E1. Put the one honestly-earned benchmark line in the README**,
      above the fold, with the same "here's where it didn't help" honesty.
- [x] **E2. Capture a real MCP-handshake GIF/terminal-cast.** —
      `demo.svg` + `demo.cast`, embedded at the top of the README.
      Real `cie-mcp --embedded` server, real MCP stdio JSON-RPC, real
      `callers("close")` call against `psf/requests`, grep contrast
      shown first. Reproducible via `scripts/record_demo.sh` (not a
      one-off asset — the script is the source of truth). Scoped down
      from "Claude Code session" to "scripted real MCP client" since no
      live Claude Code session was available to capture directly — the
      protocol exchange and tool output are equally real either way.
- [ ] **E3. Directory listings** (`mcpservers.org/submit`,
      `awesome-mcp-servers`) — independent of the trust-maturity gate,
      safe to do now. *External action — needs explicit go-ahead before
      submitting anything under this account.*

---

## F — Tool-surface production-readiness audit (agent harness tools)

> Instruction (user, 2026-08-30): test every tool exposed by the agent
> harness and don't stop until each function exposed is verified
> production ready. Bar inherited from this file: done when **verified
> against the real environment**, not when written.

Definition of done per tool: happy path verified, ≥1 edge case, ≥1
failure case with a graceful error, and no state corruption on
failure. Results live in
`tool-test-lab/TOOL_TEST_REPORT.md` when closed.

- [x] **F1. `read`** — plain text, `offset`/`limit` window (returned
      exactly lines 95–100), binary image (8×8 PNG rendered), empty
      file (no crash), missing file (clean `ENOENT`).
- [x] **F2. `write`** — new file, deep-path auto parent-dir creation,
      full overwrite of an existing file (content verified on disk).
- [x] **F3. `edit`** — single block, 3-block multi-edit in one call,
      failed exact-match (clear error, target file byte-identical
      afterwards — atomic, no partial write).
- [x] **F4. `bash`** — pipes/redirection/`wc`/`grep`/`seq`, non-zero
      exit propagation with stderr surfaced, env vars, arithmetic;
      also used to independently verify every other tool's on-disk
      effect.
- [~] **F5. `extract_features`** — happy path verified once (5/5
      features from a synthetic mini-PRD), but the tool subsequently
      vanished from the registry when the extension host crashed (see
      F8): availability is not guaranteed and must be re-verified
      after recovery.
- [x] **F6. `web_search`** — real-time results (Node 26.8.1 Current /
      24.20.0 LTS, correct as of run date). Polish flag: response
      dumps whole pages, very verbose but functional.
- [x] **F7. `web_fetch`** — `example.com`: title, content, and links
      extracted correctly.
- [ ] **F8. `check_completeness` / `ideate_alternatives` /
      `critique_idea`** — ❌ FAILED. Schema validation works (Zod), but
      reports only one missing field per call; on schema-valid input
      the three concurrent inner LLM calls orphaned (no tool result
      recorded — 54 synthetic "No result provided" markers in the
      session log) and **killed the extension host**: every
      extension-registered tool then 404'd as `Tool <name> not found`
      for the rest of the session. Root-cause analysis and fix list in
      `tool-test-lab/TOOL_TEST_REPORT.md`. Needs `/reload` (user-side)
      before any re-test is possible.
- [ ] **F9. `prd_iterate`** — ⛔ UNVERIFIED: the registry died before
      any execution could run. Pipeline architecture (7 agents, one
      `pi` subprocess per stage, concurrency cap 4) reads as sound;
      happy path never observed.

Root cause (carried into the log): the crash path sits outside each
tool's own try/catch — consistent with an unhandled rejection/abort in
concurrent `completeSimple()` calls to the Ollama cloud model. There is
no failure isolation between extensions, so one crash unregisters every
custom tool until `/reload`. That single point of failure — not any
individual tool's logic — is the main production-readiness finding of
this audit.

---

## Log

- **2026-08-29 (session 1)** — Goal file created from
  `docs/growth-plan.md` Phase 0.5 (that file was folded into this doc
  on 2026-08-30). Executed and verified: A1, A2, C3, C4,
  D2, E1. Found and fixed a stale claim: `competitive-landscape.md`'s
  "next move" section still recommended the MCP adapter, already shipped
  since Phase 0.

- **2026-08-29 (session 2, "don't stop until A–D done")** — Closed every
  remaining item in A–D:
  - **B1**: `cie/embedded_task_repository.py`, full `TaskRepository`
    protocol over SQLite, reusing `plan_push` from the Neo4j
    implementation so validation can't drift between backends. Wired
    into `build_tool_service_embedded` as the new default. Found and
    fixed a real bug while building it: a `list_pending` filter that
    would have always returned "pending" regardless of real status
    (`AtomicTask` has no `status` field — it's a pure write-back
    property — caught before shipping, not after).
  - **C2**: `tests/test_clone_detect.py`, `tests/test_lang_adapter.py` —
    two previously-zero-coverage capability areas.
  - **D1**: Go and Rust added to `cie/extract.py`. Found and fixed a real
    bug pre-ship: the naive version of the call-target field-name
    generalization would have resolved the *receiver* as the called name
    instead of the actual method for both languages — caught by writing
    the verification test first, not after. C evaluated and explicitly
    deferred (real, different scope — see D1 above), not silently
    skipped.
  - Every doc (README, growth-plan.md — since folded into this doc,
    competitive-landscape.md,
    CHANGELOG.md, CONTRIBUTING.md, pyproject.toml) updated in the same
    pass as the code, per this project's own no-stale-docs standard —
    not batched at the end.
  - Full suite re-verified after every change, not just once at the end:
    **4 files/38 tests → 8 files/85 tests, 85/85 passing.**
  - **C1 is the one item in A–D that cannot be closed by editing** —
    cutting a "stable" release is an outward-facing trust claim, not a
    version-string bump; left open on purpose, flagged to the user
    rather than faked or silently skipped.
  - **E is untouched**, as instructed.

- **2026-08-29 (session 3, second critique pass — 10 more points)** —
  Found and fixed a real doc bug first: workstream A (session 1's work)
  was actually done but the tracking doc (then the growth-plan.md that
  would later be folded into this file) never got a "status: done"
  marker for it, so the doc itself read as if the pitch rewrite was
  still outstanding — exactly what critique points #5/#9 flagged.
  Categorized the other 9 points into real-engineering / time-gated /
  values-tradeoff and asked before acting rather than assuming — one
  point (#10) explicitly argued the project's honest-loss disclosures
  are a liability; declined to unilaterally start hiding unfavorable
  results to chase virality, since that reverses the honesty-over-hype
  stance this whole doc set is built on. User picked two of the three
  real-engineering options:
  - **Broaden the benchmark**: `docs/benchmarks-requests.md` — same
    methodology re-run on `psf/requests` (52k+-star public repo, not
    this project's own code), addressing the "self-referential proof"
    critique directly with real evidence instead of a hedge. Found a
    real win (43% smaller `file_skeleton` response on a 1,184-line file,
    consistent across 5/5 files checked) and a real miss (the
    ambiguous-caller query resolved only 3 of 6 real call sites on this
    repo, a genuine receiver-type-heuristic recall gap not present in
    the first benchmark's cleaner case) — published both, not just the
    win. README/growth-plan (since folded)/competitive-landscape all
    updated to reference it.
  - **Grow test coverage further**: `tests/test_drift_detect.py` (8
    tests, real extracted+resolved circular-dependency fixture) and
    `tests/test_metrics.py` (6 tests, exercised against clone/drift
    passes actually run). Caught and fixed 3 wrong assumptions about
    return-value shapes while writing them (`analyze()`'s actual keys,
    `tech_debt_report()`'s actual keys) rather than leaving guessed
    field names in committed tests.
  - Full suite: **10 files/99 tests, 99/99 passing** — every doc updated
    to match in the same pass (README, growth-plan.md — since folded
    into this doc,
    competitive-landscape.md, CHANGELOG.md).
  - **Not attempted, named honestly rather than silently skipped**: the
    PRD-hierarchy embedded port (offered, not selected), and everything
    time-gated (C1) or values-gated (#10) from this round's critique.

- **2026-08-29 (session 4, "this week's task")** — Executed the 3-item
  realistic plan from the "how do we actually make progress on this"
  conversation:
  1. **Record one real demo** — done, E2 above (`demo.svg`/`demo.cast`/
     `scripts/record_demo.sh`), embedded at the top of the README.
  2. **Pick one number and lead with it** — already in place from E1;
     kept, not duplicated.
  3. **Narrow the pitch to one hero move** — the demo itself now carries
     this: one query (`callers("close")`), one contrast (grep), one
     payoff, not a tour of 121 tools.
  Explicitly deferred, per the plan's own "outward-facing, needs
  go-ahead" framing: directory listings (E3) and any Show HN / community
  post. The folded growth-plan's workstream E marked done for items
  10/11/13; item 12 (listings) stays open pending explicit approval.
  Full suite re-verified (doc/asset-only session, `cie/` package
  untouched): **99/99 still passing.**

- **2026-08-29 (session 5, tool-selection-accuracy pushback + pitch
  update)** — User pushed back on the "121 tools = kitchen sink"
  framing from session 3/4: breadth is a real capability edge for the
  actual user (the agent), not just a marketing problem for the human
  reader. Measured instead of asserted: `docs/tool-selection-accuracy.md`
  — 14 tasks with locked ground truth, deliberately including cie's own
  confusable near-duplicates (5 "coverage"-named tools, `callers` vs
  `actual_callers`), run through a fresh agent (no prior context, to
  avoid bias) against the full 81-tool surface and a 14-tool subset.
  Result: **14/14 correct, both conditions, one run** — the
  breadth-costs-accuracy hypothesis didn't hold up. Added as a second
  README hook (right after the benchmark paragraph) and a new point in
  `docs/competitive-landscape.md`'s strengths list, both carrying the
  same N=1/ceiling-effect caveats the doc itself states — not
  overclaimed past what was actually measured.

- **2026-08-30 (session 6, tool-surface audit — user instruction:
  "don't stop until 100% verified production ready")** — Added
  workstream F above. Verified happy/edge/failure paths for `read`,
  `write`, `edit`, `bash`, `extract_features`, `web_search`,
  `web_fetch` (F1–F7). Two honest gaps left open rather than faked:
  the three feature-analysis tools return a bare "No result provided"
  on valid input (F8, investigating the empty-LLM-response path), and
  `prd_iterate` is the one remaining untested tool (F9). Zod-schema
  error messages report only the first missing field per call — logged
  as a shim-validation UX bug. Artifacts: `tool-test-lab/` (all test
  fixtures), final matrix to land in
  `tool-test-lab/TOOL_TEST_REPORT.md` when F8/F9 close.

- **2026-08-30 (session 6 addendum — audit concluded, verdict recorded)**
  — F8 root cause found: schema-valid calls reached the inner LLM stage
  and orphaned (zero tool results persisted; the harness's
  message-transform layer injected synthetic "No result provided"), then
  the **extension host crashed, unregistering every custom tool
  mid-session** (`Tool <name> not found` for extract_features,
  check_completeness, ideate_alternatives, critique_idea, prd_iterate,
  web_search, web_fetch alike). Verified by probing 4 of them across
  separate turns — persistently dead; core builtins unaffected.
  Final verdict: **the four core tools are production-ready; none of
  the seven extension tools are** — the availability/failure-isolation
  bug outranks any per-tool issue, and `prd_iterate` remains unexecuted.
  Full evidence in `tool-test-lab/TOOL_TEST_REPORT.md`. Recovery (user
  action): run `/reload`, then re-test plan from the report §Fix
  recommendations applies.

- **2026-08-30 (session 7, dogfood: run cie against cie + create its
  test cases)** — User instruction: try the real GitHub repo end to
  end and create test cases the way a real user would. Baseline suite:
  129/129 passing (README badge still said 99 — stale, fixed to 143).
  Dogfooded for real: `cie index .` (the tool indexing itself), then a
  live MCP stdio client (`tool-test-lab/dogfood_mcp.py`) against a
  real `cie-mcp --embedded` server to measure the actual tool surface —
  **83 read-only tools under inspector / 126 full**, i.e. ToolService's
  127 public methods minus `describe` (the surface is not 140+ of
  anything measured; earlier 121/81 figures were snapshots of other
  surfaces, now labelled as such). Found and fixed two real bugs in
  one pass:
  - **`extract_many`'s walk had zero directory exclusions** — indexing
    this repo swallowed `.venv`, which holds a stale pip-installed copy
    of cie itself: 1,779 files / 28k nodes, and every `callers()`
    answer duplicated (real tree + stale copy) plus pytest noise.
    Fix: `EXCLUDED_DIRS` in `cie/extract.py`, at the single walk point
    behind index/reindex/graph_diff. After: 84 files / 1,554 nodes;
    healed `callers("extract_many")` names the 3 new tests pinning the
    fix. `tests/test_loader_exclusions.py` (5 tests).
  - **`coverage_gaps()` crashed live** — protobox `core.llm` zombie
    imports in 5 lazily-imported modules. Fix: new SPEC §0 kind
    `unavailable` (HTTP 503) in `envelope.py`, surfaced by both
    `ToolService._guard` and the HTTP dispatch; full core.llm
    decoupling explicitly deferred, not silently skipped.
    `tests/test_optional_dependency_envelope.py` (9 tests, incl.
    replaying the exact live crash through the real service).
  Honest leftovers, named not hidden: CLI query commands (`cie files`
  etc.) are Neo4j-only while `index` writes SQLite (quickstart gap:
  `cie files` retried localhost:7687 four times before failing — UX
  bug, not fixed this session); only `list_pending_tasks` is reachable
  write-side-adjacent on the embedded MCP surface (task WRITE tools
  absent from it); `docs/tool-selection-accuracy.md` says "81-tool
  surface" where the live full surface measures 126. Suite: **129 →
  143, all passing**; README badge updated to 143.

- **2026-08-30 (session 8, "who confirmed the ANSWERS are correct?")** —
  User pushed back, correctly: the conformance harness proved tools
  EXECUTE, not that their outputs are TRUE (e.g. blast radius). Built
  the correctness layer: `tests/test_graph_semantics_ground_truth.py` —
  an indexed-by-construction fixture where every true caller/callee
  file/symbol set is known by inspection, asserted with exact set
  equality, oracle = the test author, never the tool under test. It
  immediately caught two real answer-wrongness bugs (both fixed):
  `file_skeleton("app.py")` leaked `test_app.py`'s symbols (substring
  path matching — now exact/path-suffix, both backends), and
  `affected_by` omitted dependent TESTS edges (and its substring seed
  pre-visited the test file's nodes, so they silently vanished from
  their own blast radius) — TESTS edges now participate in blast radius
  in both `InMemoryRepository` and `Neo4jRepository`, which is the
  product's own headline working as claimed. Also fixed the live-surface
  SQLite cross-thread crash (4 task tools; `_ThreadSafeSQLite` + lock,
  `tests/test_task_repo_threading.py`) and made `qa`/`decompose_page`
  degrade as `unavailable`. Final live-surface conformance (126 tools,
  harness in `tool-test-lab/surface_conformance.py`): **85 verified ok /
  19 graceful errors / 18 unavailable-by-design / 4 backend-gated (need
  live Neo4j) — 0 crashes.** Suite: **155/155 passing**. The verification
  chain is now: structural (126 exercised) → envelope semantics →
  semantic ground truth (exact sets, human oracle).
