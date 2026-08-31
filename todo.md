# todo.md — executable plan for every roadmap item

> Created 2026-08-30. Companion to `roadmap.md` (what + why) and `goal.md`
> (process state). Every item below was planned **against the actual code
> on this commit** — files, line references, and ripple-effects are from
> a real read of the tree, not guesses. Survey inputs: `cie/routes.py`,
> `cie/tools/__init__.py`, `cie/mcp_server.py`, `cie/tool_policy.py`,
> `cie/cli.py`, `cie/factory.py`, `cie/embedded_repository.py`,
> `cie/hierarchy.py`, `cie/extract.py`, `cie/callgraph.py`,
> `cie/tools/heuristic.py`, `tests/*`, `tool-test-lab/*`,
> `.github/workflows/ci.yml`, README/CHANGELOG/docs.
>
> Baseline when planned: 171/171 tests; live conformance 126 tools —
> 85 verified / 19 graceful / 18 unavailable-by-design / 4
> backend-gated / 0 crashes; `ToolService` introspects to exactly **126
> public methods** (verified live via `vars(ToolService)`).

**Ground rules (inherited from roadmap, non-negotiable):**
1. DoD = *verified against the real environment*, never written-only.
2. Every item that changes a tool count or a behavior re-measures and
   updates every citable number **in the same pass** (the no-stale-docs
   rule) — see the Count Contract below.
3. P2 items stay gated on R6. No launch mechanics first.

---

## 0. Execution order and dependency graph

```
R4 (doc archaeology, S) ──────────┐
R3 (count sweep — see note) ──────┤
R5 (shrink unavailable, M) ───────┼──► R6 (cut 0.1.0 stable — the C1 gate)
R2 (CLI↔SQLite parity, M) ────────┤        │
R1 (MCP write-side parity, M) ────┘        │
                                           ▼
R14 (hierarchy→SQLite, M) ◄── chore: fix hierarchy.py stale docstring
     │ (do before/with R1's hierarchy tier, see below)
     ▼
P1 in trust order: R7 → R8 → R9 → R10 → R11 → R12 → R13
                                           │
P2 (gated on R6):            R15 → R16 → R17 → R18 → R19 → R20
```

Recommended working order within P0: **R4 → R2 → R1(+R14 seam) → R5 →
R3-final → R6**. Rationale: R4 is pure docs and removes a broken link
everything else cites; R2 is required for R3's "every cited number
matches pytest and the live surface" verify to even make sense on the
zero-config path; R1 grows the surface (count changes once, here); R5
shrinks the unavailable bucket (count changes again); R3's *final* sweep
runs last-before-R6 so the release ships with counts measured on the
release candidate commit, not a pre-R1 desktop.

**The Count Contract (cross-cutting, applies to R1/R2/R5/R6/R12/R13):**
the single source of truth for "how many tools" is introspection, never
prose: `len([m for m in vars(ToolService) if not m.startswith('_') and m
!= 'describe' and callable(...)])` (measured 126 today), plus the
surface-label convention from CHANGELOG L123–127 (126 ToolService /
read-only subsets per policy). Any PR that adds/removes/moves a tool
must, in the same diff: update README's "~N tools" + badge claims,
`docs/tool-selection-accuracy.md`, `docs/competitive-landscape.md`,
`docs/language-agnostic-design.md`, CHANGELOG (new dated entry — history
entries are never retro-edited), and re-run `tool-test-lab/
surface_conformance.py`, committing the fresh `surface_results.json` as
an artifact of record.

---

## Cross-cutting impact map (who touches what)

| Shared file/table | Touched by | Why it matters |
|---|---|---|
| `cie/routes.py` (`TOOLS`, `HTTP_WRITE_ALIASES`, `_authorize_http_tool`) | R1, R14, R16 | alias handlers → ToolService moves authorization path from alias-set to `WRITE_TOOLS` |
| `cie/tool_policy.py` (`WRITE_TOOLS`) | R1, R14, R16 | promoted write-alias handlers MUST land in `WRITE_TOOLS` or the read-only HTTP default silently allows writes — the one genuinely dangerous ripple in the whole plan |
| `tests/test_tool_surface_invariants.py` (`HTTP_ONLY_HELPERS`, policy tests) | R1, R14, R17 | these tests pin the three-front-end parity contract; they are the gate that must be *updated*, not dodged |
| `cie/cli.py` (`_open_engine`, `_open_task_repo`, `_open_hierarchy_repo`, `_open_tool_service`, group docstring "query a knowledge graph stored in Neo4j" L419) | R2, R8, R14, R15 | one backend-dispatch seam, built once in R2, reused by R8/R14/R15 |
| `cie/factory.py` (`build_tool_service_embedded`) | R1, R2, R14 | the single construction path embedded mode already funnels through |
| `cie/extract.py` (`_LANG_LOADERS`, `_FUNCTION_TYPES`, `_CALL_TYPES`, name/param/heritage dispatch) | R12, R13 | per-language tables + one new declarator-unwrap helper |
| `pyproject.toml` (deps, version, classifiers) | R6, R12, R13 | tree-sitter-c/cpp/c-sharp deps; 0.1.0 bump |
| `README.md`, `docs/*.md`, `CHANGELOG.md` | everything | Count Contract + per-item doc anchors listed inline below |
| `tool-test-lab/surface_conformance.py` + `surface_results.json` | R1, R5, R6, R17 | classification buckets + reason assertions + the hardcoded `PYTHONPATH` pin (​L107) must be parameterized before CI can run it |
| `.github/workflows/ci.yml` | R6, R17 | conformance job + release job |
| `scripts/record_demo*.sh` pattern | R8, R15, R20 | record-then-commit, not one-off screenshots |

---

## P0 — before 0.1.0 stable can honestly cut

### [x] R1 · Embedded MCP write-side parity (M) — DONE 2026-08-30

*(Implementation summary: six `cie.routes._tool_*` handlers ported
verbatim into ToolService methods; routes dispatch via
`_service_tool`; `WRITE_TOOLS` +6 in the same commit with the overlap
pinned by `test_http_write_aliases_and_write_tools_never_overlap`;
`HTTP_ONLY_HELPERS` −6; policy 403-tests per promoted name; 19-test
`tests/test_task_qa_write_parity.py` on the embedded backend incl.
SQLite on-disk verification; live MCP stdio probe executed all six;
surface 126→132, conformance re-run committed.
`push_hierarchy` + read-side hierarchy helpers wait for R14. Full
detail in the commit and CHANGELOG [Unreleased].)*

**State today (verified).** `cie/routes.py` implements 7 task/QA write
tools as bespoke `_tool_*` HTTP handlers — `_tool_push_tasks` (L374),
`_tool_set_task_status` (L399), `_tool_link_artifact` (L431),
`_tool_append_repair_events` (L463), `_tool_record_coverage` (L519),
`_tool_record_coverage_snapshot` (L619), `_tool_push_hierarchy` (L691) —
registered in the `HTTP_WRITE_ALIASES` frozenset (L145) and authorized
via the alias-set branch of `_authorize_http_tool` (L167). The MCP server
(`cie/mcp_server.py::build_mcp_server`) only introspects `ToolService`
public methods, so these 7 never appear on the default `cie-mcp
--embedded` install — the headline task/QA layer is read-only there.
`tests/test_tool_surface_invariants.py::HTTP_ONLY_HELPERS` currently
documents this split as *allowed* (17 names). Embedded side already has
the backends these tools need: `EmbeddedTaskRepository` (task write
side) and `EmbeddedRepository`/`InMemoryRepository.record_coverage*`
(L1207–1312). The only seam missing for embedded is the hierarchy repo
— see R14.

**Plan.**
1. **Promote, do not re-implement.** Move each `_tool_*` handler's body
   (validation + partial-accept semantics + hint strings) into a real
   `ToolService` method with the **same kwargs and same envelope
   shape**, one commit per tool family: (a) `push_tasks`;
   (b) `set_task_status`, `link_artifact`, `append_repair_events`;
   (c) `record_coverage`, `record_coverage_snapshot`. In
   `cie/routes.py::TOOLS`, replace each bespoke entry with
   `_service_tool("<name>")` so the HTTP contract (URL, kwargs, envelope,
   "Field required" retry hints the conformance harness relies on) is
   bit-identical. Methods resolve their backends through
   `self._task_repo` / `self._engine` exactly the way the handlers use
   `factory.get_task_repo(project)` / `factory.get_engine(project)` —
   no new wiring.
2. **Authoritative trap — do this before anything else can ship:** add
   all 6 task/QA write names above to `cie/tool_policy.py::WRITE_TOOLS`.
   The moment they become ToolService methods, routes' `_authorize_http_tool`
   will classify them via `authorize()`/`WRITE_TOOLS`, NOT
   `HTTP_WRITE_ALIASES` — if they're not in `WRITE_TOOLS`, the default
   read-only HTTP policy (INSPECTOR) would silently start permitting
   writes. Then shrink `HTTP_WRITE_ALIASES` to empty (or only
   `push_hierarchy` pending R14) and add an invariants test pinning:
   `HTTP_WRITE_ALIASES ∩ WRITE_TOOLS == ∅` **and** every name removed
   from the alias set is present in `WRITE_TOOLS` — so the trap can't
   recur next time a handler is promoted.
3. `push_hierarchy` (+ read-side `get_children`, `get_lineage`) needs a
   hierarchy backend on embedded — promote these **in R14's PR**, not
   here; R1's verify scope is the task/QA surface that stands on
   `tasks.db` alone.
4. After promotion, update `tests/test_tool_surface_invariants.py`:
   `HTTP_ONLY_HELPERS` shrinks by 6 (the remaining honest set becomes
   `health`, `schema_version`, `validate_api_contracts`,
   `validate_coverage`, `validate_cycles`, `get_coverage`,
   `coverage_report`, `coverage_trend` — all read-side or HTTP-surface
   helpers); re-run the whole file and *read the failure list before
   editing* — that list IS the parity contract.
5. Re-run and commit fresh artifacts: `tool-test-lab/
   surface_conformance.py` (new surface = 126 + 6 = **132** tools;
   expect assigned `verified`/`graceful` for the promoted writes against
   `.cie/tasks.db`) and `tool-test-lab/dogfood_mcp.py` (its probe list
   should now find push-side names).

**Impact / ripples elsewhere.**
- `cie/tool_policy.py` docstring says its classification was read
  2026-08-28 and "keep this set in sync by hand" — amend that comment
  and the `HTTP_WRITE_ALIASES` comment block on routes L139–146, which
  will otherwise lie after promotion.
- Count Contract: README "~126", `docs/tool-selection-accuracy.md` and
  `docs/competitive-landscape.md` numbers → measured 132; note
  `docs/tool-selection-accuracy.md`'s 14/14 claim was gathered on a
  *different* surface — re-run its harness or re-label the claim's
  surface as the 08-30 one (decide in PR).
- `CHANGELOG.md`: new Added/Fixed entry; historical entries untouched.
- No `mcp_server.py` code change (introspection does the right thing
  automatically) — that's the point of the fix — but its module
  docstring's "~121 methods" comment needs the new number.
- `http_policy` tests (`tests/test_http_policy.py`): write-refusal cases
  for the 6 promoted names must still 403 under default policy — add
  explicitly, one per tool.

**Verify (from roadmap, made concrete).** `python tool-test-lab/
dogfood_mcp.py <indexed-root>` sees and executes `push_tasks`,
`set_task_status`, `link_artifact`, `append_repair_events`,
`record_coverage*` against `<root>/.cie/tasks.db`; invariants tests
green with the *smaller* `HTTP_ONLY_HELPERS`; conformance JSON has 0
crashes. Done ≠ merged; done = fresh `surface_results.json` committed.

---

### [x] R2 · CLI↔SQLite parity for the quickstart (M) — DONE 2026-08-30

*(Implementation summary: one backend-selection seam in cie/cli.py —
`--backend`/`--db`/`CIE_BACKEND`/`CIE_DB`/auto-probe; embedded branches
in `_open_engine`/`_open_task_repo`/`_open_hierarchy_repo`/
`_open_tool_service`; honest `unavailable`/`not_found` envelopes for
hierarchy-on-embedded, Neo4j-only ingest commands, and explicitly
missing dbs; group-level fail-fast probe; 13-test `tests/test_cli.py`
+ clean-venv verify vs psf/requests, callers("close")=3 matching the
published benchmark. Details in the commit message and CHANGELOG
[Unreleased].)*

**State today (verified).** `cie index` writes the embedded SQLite graph
(`cie/cli.py` L438–488 via `cie.embedded_repository.EmbeddedRepository`),
but every query command routes through `_open_engine()` (L70), which
unconditionally builds `Neo4jRepository.connect(...)` — the session-7
log's four retries against `localhost:7687` are exactly this line
failing. The same hardwiring exists in `_open_task_repo()` (L112) for
`tasks:*` and `_open_hierarchy_repo()` (L134) for `hierarchy:*` (that
one blocked on R14), and `_open_tool_service()` (L145) composes them.
The good news, verified: `InMemoryRepository` (embedded's engine)
implements the *full* Repository protocol surface the CLI reads —
`get_community`/`list_communities`/`god_nodes`/`stats`/`shortest_path`/
`discover_features`/`list_files`/`failing_context`/`affected_by`/
`hybrid_search`/`semantic_search` all exist (L185–1312) — so this is a
construction-path fix, not a query-engine port. The trap to respect:
community *summary labels* and embeddings come from `core.llm`
(`cie/community_detect.py`, `cie/embed.py`) which won't exist embedded —
those commands must degrade with an envelope-honest hint, not pretend.

**Plan.**
1. **One dispatch seam, built once:** add `_open_backend(project)`
   returning `("embedded", db_path)` or `("neo4j", cfg)`:
   resolution order = `--db PATH` on the `cli` group → `CIE_DB` env →
   `.cie/graph.db` in the *project root being queried* → Neo4j env
   (`CIE_NEO4J_URI` etc.). Rewrite `_open_engine`,
   `_open_task_repo`, `_open_tool_service` to switch on that seam,
   calling `build_tool_service_embedded()` / `EmbeddedRepository` /
   `EmbeddedTaskRepository` on the embedded branch. Neo4j stays the
   explicit choice when URI env is set; no behavior change for existing
   Neo4j users (their env vars still win).
2. Every `_open_engine` call site (~25 commands) keeps working unchanged
   — that's the seam's job. Audit exceptions: `bootstrap` (explicitly
   Neo4j index-creation — becomes an error-with-hint on embedded, not a
   silent no-op), `load`/`watch`/`serve` (all remain honest about which
   backend they target), `schema:dump`/`schema-version`/`health`/
   `stats` (must answer on embedded).
3. Fix the lies in prose while there: group docstring L419 ("query a
   knowledge graph stored in Neo4j") and each help string that says
   Neo4j-only.
4. Fill the honest answers: commands whose underlying data cannot exist
   embedded (embeddings → `semantic-search`; LLM community labels →
   `community`) must return the standard error envelope with a
   machine-readable hint naming the missing optional backend — reuse
   the R5 reason-string convention below so R2 and R5 ship one
   convention, not two.
5. Tests: there is **no `tests/test_cli.py` today** (verified) — R2 adds
   one: CliRunner-based, monkeypatching nothing, running `index` → each
   documented query command against a tmp project; plus a
   `test_standalone_smoke.py`-style clean-venv check for the exact
   quickstart sequence.

**Impact / ripples elsewhere.**
- `README.md` Quickstart gains 2 lines ("query it from the CLI: `cie
  files`, `cie path` … — same `.cie/graph.db`"); `docs/language-
  agnostic-design.md` backend-matrix section updated.
- `docs/competitive-landscape.md` honest-gaps: "CLI requires Neo4j" is a
  known caveat — update the entry in the same PR (no-stale-docs rule).
- `_close_engine`/`_close_driver` teardown paths must handle the
  embedded branch (SQLite conn has no driver; `EmbeddedRepository` wants
  an explicit `close`/flush — it flushes per-call today, fine).
- R8 and R14 depend on this seam; R15's `cie init` command schemas too.

**Verify (from roadmap).** Fresh venv: `pip install -e .` → `cie index
.` → `cie files .`, `cie search-symbol`, `cie path`, `cie callers`,
`cie callees`, `cie skeleton`, `cie view-file`, `cie stats`, `cie
communities` each answer against `.cie/graph.db` (or return an honest
envelope-error naming the missing optional backend — no bolt traffic at
all, proven by `--db` pointing into an empty dir and zero sockets).

---

### [x] R3 · Stale-surface sync, repo-wide (S) — DONE 2026-08-30

*(Done as the post-R1/R2/R5 final counts pass: badge 155→213; live
citations re-measured (132/83, introspection + conformance artifact);
dated snapshots labeled per the convention; durable tool-count-label
block added to README; competitive-landscape and language-agnostic-
design counts updated; CLI command count corrected 49→47, verified by
walking the real click tree; grep sweep clean — remaining "81" mentions
are explicitly dated snapshot labels; dated CHANGELOG/competitive-delta
records stay frozen per convention.)*

**State today (verified).** Live stale spots: README L10 badge
`tests-155 passing`; README L66 `81-tool surface` hook paragraph;
`docs/competitive-landscape.md` L85 (81-tool); `docs/tool-selection-
accuracy.md` L19 + L56 (81 tools); `docs/language-agnostic-design.md`
L194 (~121). CHANGELOG L123–127 already states the correct convention
(126 MCP tools = 127 public methods − `describe`; 83 read-only under
`inspector`). Historical CHANGELOG entries (L115, L243, L270, L320) are
*record*, not claims — do not retro-edit them.

**Plan.** Sections follow. This is deliberately the LAST P0 item before
R6, after R1/R5 have moved the numbers, and it re-measures everything
from the live code:
1. Measure once, cite everywhere: test count from `pytest -q` (171 as
   of planning, re-run); surface counts from introspection + a fresh
   `surface_conformance.py` JSON; policy subsets from
   `count: 126 − len(WRITE_TOOLS ∩ surface)` per policy.
2. Apply the **surface-label convention** (CHANGELOG's) as a short
   labeled table pinned in README next to the tools section, and make
   every doc citation use a label ("N ToolService tools / M over MCP
   under `full` / K read-only under `inspector`") instead of a bare
   number, so the drift class dies structurally, not just this once.
3. Update the README badge to a number, and — to kill the drift
   permanently — note in `README` that the badge is re-cut each release
   (R6 checklist step), not CI-live.

**Impact / ripples.** None code-wise. Everything count-citing elsewhere
(roadmap.md header baseline, goal.md log lines) is historical record and
stays. R6's verify re-asserts all of it on the release commit.

**Verify (from roadmap).** `grep -rn "155 passing\|81-tool\|~121"`
returns only historical/change-record contexts (dated CHANGELOG entries,
session logs); every live citation matches that commit's `pytest -q`
count and `tool-test-lab/surface_results.json`.

---

### [x] R4 · Reconstruct the provenance of the plan docs (S) — DONE 2026-08-30

**State today (verified).** `docs/growth-plan.md` does not exist and has
**no git history** (checked `git log --all --diff-filter=A`).
`goal.md` referenced it at 11 places and `roadmap.md` L8 cites it too.
Conclusion: **fold, don't restore** — there's nothing to restore from
history. *DONE 2026-08-30:* tombstone written; goal.md gained a
Provenance section; every reference rewritten with folded language.
(Plan body below retained for the record.)

**Plan.**
1. Deep-read the remaining live content goal.md actually inherited (its
   workstreams A–F already carry the substance). Anything growth-plan
   had that goal.md doesn't (the "Open question" section C1 cites at
   L61) gets lifted verbatim into a `## Appendix — folded from
   docs/growth-plan.md` section of goal.md, quoted and labeled.
2. Rewrite each reference from "see docs/growth-plan.md Phase 0.5" to
   "folded into this doc's <section> (formerly docs/growth-plan.md
   Phase 0.5)". Add a one-line provenance note at goal.md's top.
3. Add a permanent tombstone `docs/growth-plan.md` stub (5 lines:
   "folded into ../goal.md on 2026-08-30; do not recreate; history has
   no copy") so external links/deep links from old sessions land
   somewhere honest instead of 404.

**Impact / ripples.** `roadmap.md` L8's provenance line gets a pointer
to the appendix. `README.md`/docs do not reference growth-plan
(verified grep) — no other links to repoint.

**Verify (from roadmap).** `grep -rn "growth-plan"` → only
folded/restored-convention text; the tombstone exists; every goal.md
internal link resolves.

---

### [x] R5 · Shrink the unavailable-by-design surface (M) — DONE 2026-08-30

*(Implementation summary: lazy `core.llm` in 4 modules → 13 tools
un-503'd; `err` envelope gains optional `error.reason` slug; `_guard`
classifies ModuleNotFoundError/DetectorUnavailable into
`OPTIONAL_BACKEND_MISSING:*` / `HOST_PLUGIN_MISSING:*` slugs; registry
pinned in `tests/test_unavailable_reasons.py` incl. a regrowth gate;
conformance harness carries reason into `surface_results.json` — live
run: 132 tools / 100 verified / 23 graceful / 5 unavailable. Real crash
found+fixed en route: `failing_context("")` IsADirectoryError via the
heuristic fallback.)*

**State today (verified).** The 18-tool unavailable bucket decomposes,
by module, to exactly six root causes — all module-level imports of the
host-project `core.llm` (or plugin contract) executed when a tool lazily
imports its feature module: `cie/community_detect.py` L45 (→
`community_detect_run`, `community_search`, `community_summarize_run`),
`cie/contracts.py` L30 (→ `contracts`, `contracts_run`, `validate_types`
— shares the module), `cie/graphrag.py` L32 (→ `qa`),
`cie/state_machine.py` L20 (→ `state_machine_run`, `fsm_validate`),
`cie/test_orchestration.py` L40 (→ `run_tests`, `test_plan`,
`test_results`, `coverage_gaps`, `unified_coverage_report`,
`nook_and_corner_test`, `inject_assertions`, `strip_assertions`),
plus `decompose_page` (`DetectorUnavailable` plugin contract). The 4
backend-gated tools (`implied_pages_run`, `prd_traceability_*`) are a
different bucket: their engine methods have no InMemoryRepository
implementation yet (stretch: porting them to embedded, InMemoryRepository
already has `record_coverage` family at L1207+, collapses them into
verified — good R5 side-quest, not the gate).
`ToolService._guard` (L~245) already maps `ModuleNotFoundError` →
`kind=unavailable` with a prose hint; conformance classifies on
`kind=unavailable` (surface_conformance.py `classify`, L82–84).

**Plan.** Take option (a) *decouple*, not option (b) plugin-surface, for
these six — they're data/analysis routines whose LLM use is one call
inside a pipeline, and the plugin surface is already the decompose
pattern. Concretely, per module:
1. **Defer imports, gate capabilities.** Module-level `from core.llm
   import ...` moves into the single function that actually calls the
   agent; the module's *pure* layers (community graph algorithms,
   contract parsing, FSM text parsing, test-plan heuristics) run and
   return real results with `llm_summary: null` + a reason instead of
   the whole tool 503-ing. Each ToolService method then returns
   `ok=true` with a results-shaped "degraded" payload — verified/graceful
   in conformance — or, for tools that are *only* an LLM call
   (`qa`), a crisp `unavailable` with a machine-readable reason.
2. **Machine-readable reasons.** Add `reason` to the error envelope:
   `_err` grows an optional `reason: str` slug next to
   `kind`/`message` (SPEC §0 change — bump `envelope.py`'s version
   note). Slugs: `OPTIONAL_BACKEND_MISSING:core.llm`,
   `HOST_PLUGIN_MISSING:decompose`, `NEO4J_REQUIRED`, etc. Pick the
   slug per call site; `_guard` fills it automatically from the
   exception context.
3. **Pinned registry.** New `tests/test_unavailable_reasons.py`:
   a hand-curated dict `{tool_name → reason_slug}` asserted per tool
   against a live embedded ToolService — the "each with its reason
   asserted in the harness" requirement, in pytest form, so
   `surface_conformance.py` (which should also carry the reason through
   to `surface_results.json`) can diff against it.
4. Re-run conformance → commit new `surface_results.json`; expected
   end-state: unavailable ≤ 6, and each remaining one's `reason` field
   is the asserted contract.

**Impact / ripples elsewhere.**
- `ToolService._guard` + `cie/envelope.py`: error envelope shape grows a
  field — check consumers: routes `_service_tool` passthrough (fine),
  MCP server content serialization (dict passthrough, fine),
  conformance `classify()` gets a `reason` extraction line.
- `tests/test_optional_dependency_envelope.py` — extend with reason
  expectations; it's the pre-existing test of this navigation.
- Count Contract: 88→~95 verified/graceful shifts every surface citation
  (README, docs) *again* — R3-final runs after this.
- `docs/competitive-landscape.md` "unavailable-by-design" honesty
  footnote updates in the same PR.
- Careful scope line: do NOT start re-implementing `core.llm` (an LLM
  client abstraction) inside cie here — that's a product decision
  (candidate for the P1 semantic work in R10), keep R5 to
  degrade-honestly + reason-strings.

**Verify (from roadmap).** Re-run `tool-test-lab/surface_conformance.py`:
unavailable 18 → ≤6; every unavailable row carries `reason` and the
registry test pins it; README's "~N tools" carries the honest footnote
naming what remains 503 and why.

---

### [x] R6 · Cut 0.1.0 stable — the C1 gate (S once R1–R5 close) — DONE 2026-08-31 (GitHub release live; PyPI distribution renamed to `cie-mcp` in 0.1.1 same day — upload runs with the maintainer's creds)

*(Implementation: release commit `a22b4bf` — version house 0.1.0a3 →
0.1.0, classifier → Beta only, README badges re-cut (status-beta,
tests-279), CHANGELOG [Unreleased] → [0.1.0] with a fresh Unreleased
stub; suite 279/279; `uv build` → sdist+wheel, twine check PASSED, entry
points verified; clean-venv rehearsal from the wheel on a fresh
psf/requests clone @ 5460f467 — index 858/1822, `cie callers close` = 3,
real MCP stdio handshake from an independent client: 85 read-only tools,
callers() executes with resolution gap visible. Tag `v0.1.0` pushed,
GH release live with the caveats block kept; C1 hand-closed in goal.md
with the tag id. **Open follow-up RESOLVED 2026-08-31:** PyPI upload
under `cie` is impossible — pypi `cie` is cluster311/cie10 (ICD-10
codes, latest 0.208); the user chose `cie-mcp` as the distribution name
and the rename shipped in 0.1.1 (import package/CLI/tags unchanged;
install lines swept across README/docs same pass — session-log pass
18). The remaining step is the maintainer-credentialed upload itself.)*

**State today (verified).** `pyproject.toml` version `0.1.0a3`,
classifier `Development Status :: 3 - Alpha`; CHANGELOG's latest dated
section is `[0.1.0a3] - 2026-08-30`; repo has 10 commits; `goal.md` C1
open and explicitly "cannot be closed by editing".

**Plan (the release itself is a checklist run, not a coding task).**
1. **Gate check:** R1–R5 checkboxes verified `[x]` with committed
   artifacts; suite green on the release commit; conformance green with
   0 crashes; every count in README/docs re-measured same-commit (this
   is R3's final sweep data).
2. Version house: `0.1.0a3` → `0.1.0`; classifiers: Alpha → Beta is the
   *predatory-reading* risk — a 0.1.0 stable at first-repo-maturity
   still ships caveats, so classifier moves to `4 - Beta` **only**.
   Status badge in README L36 (`status-alpha`) moves to beta in the
   same diff; `pyproject.toml` classifiers + version; CHANGELOG `## 
   [0.1.0] - <date>` section assembled from the unreleased P0 entries
   (Keep-a-Changelog move, not rewrite).
3. Release notes on GH: rendered from the CHANGELOG section **plus**
   the alpha-era caveats block (contributor count, single-author,
   benchmark scope honesty); KEEP the honest-loss links
   (`docs/benchmarks-requests.md` 3-of-6 line) in the notes body.
4. `python -m build` → verify sdist/wheel contain `cie/` and entry
   points (`cie`, `cie-mcp`); upload to PyPI **after** the clean-venv
   rehearsal passes.
5. Clean-venv rehearsal, third-party repo (the roadmap's own bar):
   fresh `python -m venv`, `pip install "cie[mcp]"` from PyPI, `cie
   index <third-party-repo>`, `cie-mcp <repo> --embedded` handshake via
   a real MCP client, one query command via CLI, then (R18's listings
   only *after* the artifact is live).
6. Hand-close C1 in `goal.md` **by hand** with the tag id, and cut tag
   `v0.1.0` + GH release. Only now does P2 ungate.

**Impact / ripples everywhere.** This is the step the whole tree was
prepped for: post-release, README "status" prose, `docs/competitive-
landscape.md` "alpha caveats", and `goal.md` C1 all flip in one
documented pass.

**Verify (from roadmap).** Clean-venv `pip install cie[mcp]` quickstart
against a third-party repo end-to-end; tag + release live; C1 closed by
hand in goal.md.

---

## P1 — differentiator defense + credibility (after stable)

### [x] R7 · Edge provenance tagging (L) — DONE 2026-08-30

*(Implementation summary: `callgraph.resolve_call_edges` computes per-name
total/unresolved call-site tallies in the same pass (`resolution_stats`);
both loaders persist them as `CallResolutionStat` analysis nodes;
ToolService `callers`/`callees` shapes per-row `provenance`
(graph|heuristic-name-match, including on fallback legs) + envelope
`resolution`; 4 ground-truth tests (`tests/test_edge_provenance.py`);
live psf/requests check matches the published 3-resolved with the gap
exposed (`19/16/3`).)*

**State today (verified).** The *graph* half exists: `cie/callgraph.py`
already stamps every `calls`/`extends`/`implements` edge with
`Confidence` — EXTRACTED (same-file/import-resolved, L323–333),
INFERRED (receiver heuristic, L255–278), AMBIGUOUS (L344) — and
`ToolService._edge_results` (L~600) passes `edge.confidence` through.
The *unseen* part R7 names: (a) when `callers`/`callees` fall back to
`HeuristicToolSet` (heuristic.py), results also say `INFERRED` —
indistinguishable from the graph's INFERRED — and only the envelope
hint mentions degradation; (b) *unresolved* call sites are dropped
silently at `resolve_call_edges` (L346–348: `if target is None or
confidence is None: continue`), so "3 of 6 real call sites resolved"
is invisible in tool output — it lives only in
`docs/benchmarks-requests.md`.

**Plan.**
1. **Provenance on every edge-shaped result.** `_edge_results` gains
   `provenance: "graph" | "heuristic-name-match"`, set by the caller
   (`callers`/`callees` know which path served the envelope); heuristic
   toolset results get `"heuristic-name-match"` — a viewer can now
   discount correctly. `failing_context`/`affected_by`/`path_between`
   chain/hops get the same field. `entity_context`/`serialize.py`
   passthroughs updated so one shaping exists.
2. **Make resolution completeness visible.** At extraction, count call
   sites per unresolved-name (`resolve_call_edges` already has the
   candidates) and persist a tiny per-name tally — a `NodeKind.
   FUNC`-adjacent property or a dedicated stats edge written by both
   backends at load time (repository `load_extraction` change + Neo4j
   `merge_delta` + InMemory/Embedded equivalents, one protocol method).
   `callers()`/`callees()` then add
   `resolution: {"resolved_edges": N, "unresolved_call_sites": M}` so
   the requests-gap is *in the output*. (If persisting proves
   sprawling, fall back to computing on-demand from
   `_symbol_index`/extraction call-site records — decide at
   implementation, keep the tool contract identical either way.)
3. **Ground-truth fixture test**, modeled exactly on
   `tests/test_graph_semantics_ground_truth.py` (same
   known-by-inspection-oracle pattern, lines 1–64): a fixture with one
   statically-resolvable call, one receiver-inferred call, one
   unresolvable name referenced twice — asserting `provenance` labels
   AND `resolution` counts equal the human-derived truth, on BOTH
   backends (embedded factory + mocked Neo4j repo where the suite
   already has doubles).

**Impact / ripples elsewhere.**
- `docs/benchmarks-requests.md` gains one sentence: the documented
  3-of-6 gap is now visible in `callers` output (`resolution.
  unresolved_call_sites`), and README L60–62's footnote references the
  field instead of only the doc.
- `docs/tool-selection-accuracy.md`/competitive docs untouched (no
  surface-count change; this is output enrichment, not a new tool).
- `HeuristicToolSet` envelopes gain the provenance field — check its
  consumers (czy RepairAgent weighting code in be-v2 host relies on
  exact result shape? `heuristic.py` docstring says repair loops weight
  `confidence`; additive field is safe).
- CHANGELOG entry under the next version.

**Verify (from roadmap).** Ground-truth test asserts labels against
known-by-inspection truth, both backends; fresh standalone install
returns resolution counts on psf/requests matching the published
benchmark's 3-of-6 for `close()` — the claim and the tool agree.

---

### [x] R8 · `cie export-html` — the shareable artifact (M) — DONE 2026-08-30

*(Implementation summary: `cie/export_html.py` — read-only composition
through ToolService envelopes + one project_graph fetch for chains
(node-id-exact matching); CLI `cie export-html [PATH] --out`; XSS/blob
escape tests; zero-external-reference tests; real psf/requests export
(37 files/34 chains/680 orphans/215KB/0 external refs); screenshots
committed from `file://` via scripts/record_export_html.sh. Found+fixed
en route: the JS view-activation bug the first screenshot caught
(Overview never activated).)*

**State today (verified).** Nothing HTML-export exists; the pieces it
composes do: `traceability_chain`/`traceability_orphans` (ToolService,
verified bucket), `callers`/`file_skeleton`, `build_tool_service_
embedded` as the zero-config data path, and
`scripts/record_demo.sh` + `record_demo_client.py` as the
record-then-commit pattern for artifacts (`demo.svg` precedent).
`demo.cast` shows the cast format expected.

**Plan.**
1. New `cie/export_html.py`: pure stdlib `string.Template`-built
   single-file HTML (inline CSS/JS, **no CDN, no fetch** — `file://`
   hard requirement). Data via the read-only ToolService envelopes (one
   construction path, `build_tool_service_embedded` first-class), so
   export honors exactly what the tools expose and never opens a write
   path.
2. Views in v1, centered on what nobody else renders: the task→file→
   test **traceability chains** (from `traceability_chain`), orphan
   tasks/tests (from `traceability_orphans`), per-symbol **callers**
   pane, per-file **file_skeleton** listing. A single
   pre-rendered JSON blob embedded in the HTML keeps the page static
   and client-side-searchable; no server, no auth surface — state the
   "safe slice of a viewer" boundary in the command's docstring.
3. CLI: `cie export-html [PATH] -o out.html --project CIE_PROJECT`,
   routed through R2's backend seam so the embedded path is default.
4. Screenshot artifact: extend the record-then-commit pattern —
   `scripts/record_export_html.sh` (build page for a pinned repo clone
   + headless capture), committed output joined into README next to
   `demo.svg` *as a script output, not a one-off*.
   **Impact / ripples elsewhere:**
   - README new subsection under Quickstart ("Share it: one static
     file"), CHANGELOG entry, `docs/competitive-landscape.md`
     Graphify/GitNexus rows gain "cie: static HTML export" where the
     gap was listed.
   - R9's benchmark harness can reuse the export as a task input later;
     R15's `cie init` context-file suggested snippets mention the
     command.
   - No `ToolPolicy` surface change (read-only composition of existing
     tools) — note in the PR why the surface count doesn't move.

**Verify (from roadmap).** Export psf/requests; open via `file://` in a
plain browser with zero network/deps (verify by measuring zero external
requests in the capture); screenshot committed via the script.

---

### [x] R9 · Benchmark harness + third independent repo (M) — DONE 2026-08-30

*(DONE 2026-08-30: scripts/benchmark.sh + benchmark_tasks.py; third dataset
docs/benchmarks-urllib3.md — 12 receiver-attributed caller edges vs 28 raw
grep matches, 28/40 resolution miss published, 2.24× skeleton compression;
requests numbers regenerate matching the doc, verified twice.)*

**State today (verified).** `docs/benchmarks-requests.md` is the
methodology of record (94 lines: pinned commit `5460f467`, 3 task
shapes, honest-loss section); the two-column naive-vs-cie pattern and
the 3-of-6 miss. There is no script: the numbers were a session, not a
reproducible artifact. `scripts/` holds `record_demo.sh` (the
reproducibility pattern) and `smoke_patch_mcp.py`.

**Plan.**
1. `scripts/benchmark.sh` + `scripts/benchmark_tasks.py` (Python part:
   drive `ToolService` directly against an indexed clone — the same
   calls `record_demo_client.py` makes over MCP, minus transport
   ceremony): input = `(repo_url, pinned_commit, src_glob)`; steps =
   clone → `cie index` → run the 3 canonical task shapes (`easy
   definition`, `ambiguous callers`, `large-file skeleton`) → run the
   naive-side commands too → emit a **markdown table + raw JSON**.
   Add the new metric: **token-per-query** — chars of tool-result
   payload ÷ naive-side output chars for equivalent tasks, reported
   with the methodology caveat that token counting is chars-based
   unless a tokenizer pin is chosen; cite codebase-memory-mcp's
   "120× fewer tokens" strictly as vendor-claim vs ours-measured.
2. Third repo: pick one public, Python, ≥10k-line, status-quo-famous,
   and *not* `psf/requests` — candidate: `httpie/cli` (un-
   ambiguous entry points) or `keleshev/schema`; pick on "does it have
   the ambiguous-name property" after a dry `grep` probe. Run
   end-to-end, publish the doc as `docs/benchmarks-<repo>.md`, **keep
   the honest-loss section** — pre-commit to reporting whatever
   resolves badly.
3. `docs/benchmarks-requests.md` numbers: regenerate from the script
   against the pinned commit and reconcile (if the live numbers moved,
   the doc moves with a dated re-run note — never silently).

**Impact / ripples elsewhere.** README cites two measured numbers
(1-tool vs 3-tool; 3-of-6; 43% skeleton ratio) — after regen, update in
the same PR with dated footnotes; CHANGELOG; this feeds R20's
claim-audit table directly.

**Verify (from roadmap).** A fresh-clone reader reproduces all
published numbers end-to-end from the script alone; doc's tables are
script output, pasted with a run date + commit hash.

---

### [ ] R10 · GraphRAG/embedding first-party benchmark (M)

**State today (verified).** The semantic layer is exactly two modules:
`cie/embed.py` (wraps `core.llm.embed_text`, NVIDIA NIM; standalone
falls back to a raising stub — L21–47) and `cie/graphrag.py`
(module-level `from core.llm import ...` L32 → `qa` is
unavailable-by-design in standalone, which is also why it sits in the
R5 bucket). `docs/competitor-benchmarks.md` (146 lines) holds the
version-table rigor for the 08-28 run.

**Plan.**
1. Constraint stated first: a first-party GraphRAG benchmark must run
   in an environment *with* `core.llm` (R5 did not remove that
   dependency — it only stopped it from nuking unrelated tools). Either
   run in the be-v2 environment for this benchmark, or
   (preferred, small) give `cie.embed` a first-party OpenAI-compatible
   fallback path gated by `CIE_EMBED_DSN`/NVIDIA_API_KEY so the
   benchmark is reproducible from this repo alone. Decide before
   writing the harness; document the choice in the doc's methodology
   section.
2. `scripts/benchmark_semantic.py`: fixed question set (10–20
   questions spanning exact-symbol, conceptual, and cross-file questions
   on the same corpora used in R9 — requests + the third repo),
   competitors: **claude-context**, **grepai** (each pinned by version
   + index settings table), measurements: precision@k on
   hand-labeled relevance sets, answer grounding, tokens/context bytes
   returned per query, index time. Publish **wins and misses**.
3. Land numbers in `docs/competitor-benchmarks.md` as a new dated
   section with the same tool-version table rigor; update
   `docs/competitive-landscape.md`'s "where competitors are genuinely
   ahead" GraphRAG paragraph to cite measured results either way.

**Impact / ripples elsewhere.** Competitive-landscape + README claim
sentences that currently live only as "unmeasured" caveats; possibly
`pyproject.toml` gains an `embed` extra **only if** the first-party
fallback path is chosen (dependency: `openai` client — decide then);
R20's post cites the outcome table.

**Verify (from roadmap).** Numbers in `docs/competitor-benchmarks.md`
with the version-table rigor; competitor retrieval configs stated as
vendor-documented, ours stated as measured.

---

### [x] R11 · Streamable-HTTP transport for `cie-mcp` (M) — DONE 2026-08-30

*(More existed than the roadmap implied: the parser already accepted
the transport; implemented `--host/--port` pass-through, live
verification via the official streamable_http client, wiring tests,
README/docs/CHANGELOG.)*

**State today (verified).** More exists than the roadmap implies:
`cie/mcp_server.py::_build_arg_parser` already accepts `--transport
{stdio,sse,streamable-http}` (choice wired to
`server.run(transport=...)`), and the installed SDK (`mcp 2.x`
`MCPServer.run`) literally types
`transport: Literal['stdio','sse','streamable-http']` — so the remaining
work is host/port surfacing, verification, and honest docs, not
transport plumbing. What's *missing*: no `--host/--port` args (SDK
defaults hidden), no doc line anywhere, no test, and no verification
that the inspector policy surfaces the predicted schema set over HTTP.

**Plan.**
1. Add `--host` (default `127.0.0.1` — localhost-only by default is the
   safe choice; document that binding non-loopback turns cie into a
   network service subject to the ToolPolicy boundary) and `--port`
   (default 8000), passed through to `server.run(...)`.
2. Verification harness (extend `tool-test-lab/dogfood_mcp.py` or new
   `tool-test-lab/dogfood_mcp_http.py`): spawn `cie-mcp --embedded
   --policy readonly --transport streamable-http`, connect with the
   official MCP **Inspector in browser mode** (the roadmap's named
   client) *and* an SDK `streamablehttp_client`; assert: tools/list ==
   exactly `INSPECTOR_POLICY.permits()` predictions (server-side
   filtering, never client trust), and one write-tool call attempt is
   refused server-side with the standard envelope error.
3. Tests: `tests/test_mcp_server.py` gains an in-process transport test
   (ASGI-level through the SDK's http client where feasible) so CI
   covers the flag without needing a browser; keep the browser-verify
   as a manual, recorded step.
4. Docs: README quickstart gains a 6-line HTTP block (when to prefer it,
   policy note, loopback note); `docs/language-agnostic-design.md`
   transport matrix; CHANGELOG.

**Impact / ripples elsewhere.** R16's run-tool refusal test reuses this
harness (`run` refused over HTTP is *already* the default via read-only
policy — pin it here too); tool-test-lab README documents the new
harness; security posture sentence in README (a port is now openable).

**Verify (from roadmap).** A real browser-based MCP client (Inspector)
connects over HTTP and sees exactly the schema set inspector policy
predicts; write attempts are server-side-refused; both asserted by the
harness, screenshot/cast recorded.

---

### [x] R12 · Language #7: tree-sitter C (M) — DONE 2026-08-30

*(Implementation summary: `cie/extract.py` — `.c`/`.h` loaders,
`function_definition` (shared with Python's node type, per-file language
dispatch), `_DECLARATOR_TYPES` + `_c_declarator_name` (innermost
identifier, param_list excluded — the mirror bug pinned),
`_c_params_text`/`_c_return_type_text` through the declarator chain;
pyproject `tree-sitter-c>=0.23.0`; v1 scope: no struct-classes, no
header-prototype nodes — both documented + test-pinned. 9 tests in
tests/test_extract_c.py incl. the D1 never-silently-skips guard.)*

**State today (verified).** Same tables as R12 (`_LANG_LOADERS` L72–82, `_FUNCTION_TYPES` L94–103, `_PARAM_TYPES` L105, `_CALL_TYPES`
L114) cover 6 languages; the file's own comments document the
verified-per-grammar discipline to copy. The known trap is real: in
tree-sitter-c, a function's name lives inside nested
`function_declarator` → `parenthesized_declarator`/`pointer_declarator`
paths, while the current name-extraction helpers read
`child_by_field_name("name")` off the top node (L272, L580, L731) —
field-based lookup silently returns `None` and functions vanish, which
is exactly the "naive-skip" bug class the roadmap's verify demands a
guard against. Test suite to match:
`tests/test_extract_go_rust.py` (10 tests, real parses, documented-gap
assertions) is the bar.

**Plan.**
1. `pyproject.toml`: add `tree-sitter-c>=0.23.0` to core deps (C is
   common enough to ship in-core, matching the existing six).
2. `cie/extract.py`: `.c`/`.h` loaders; **build the declarator
   unwrapper** as a shared helper `_declarator_name(node)` (walks
   `declarator`/`function_declarator`/`pointer_declarator` chains until
   an `identifier` with a name-field, returning None → skipped
   symbol with a debug trace, never a silent false-empty); wire into
   `_walk_file`'s function branch and `_collect_call_sites`. Parameter
   extraction: C uses `parameter_list` (already in `_PARAM_TYPES` —
   verified present). Calls: `call_expression` with the `function`
   field → same-file/`includes`-based resolution; decide + document the
   C scope: `#include` → import-map analog (headers as file hubs) is
   in-scope if cheap; structs/classes: **no CLASS node in v1** (match
   the Go/Rust precedent of documented non-coverage, keep `struct` as
   plain symbols, not classes).
3. **The anti-naive-skip test** (the roadmap's real ask): a fixture C
   file with functions, prototypes, pointers-returning, and nested
   declarators must assert *exact* extracted symbol sets (positive
   assertions, not just non-crash), plus one test that would fail if
   extraction ever returned empty (e.g. `assert
   {f.name for f in ...} == {...}` with a non-empty expected set) —
   style-match `test_go_extracts_function_and_receiver_methods...`.
4. Docstrings/comments: extension docs
   (`docs/adding-a-language.md`) get a "C declarator" section naming
   the trap for the next language porter (it recurs in C++).

**Impact / ripples elsewhere.** README language list + badges,
`docs/competitive-landscape.md` language-count row (6 → 7, **do not
round up further**), `docs/language-agnostic-design.md` coverage table,
`pyproject.toml`, CHANGELOG, `tests/test_extract_c.py` (new, ~8–12
tests), and the R13 three-way declarator plan. `_docstring()` needs a
C branch (best-effort attach comment blocks or honestly None — same
graceful treatment as Go/Rust's empty docstrings, test included).

**Verify (from roadmap).** `test_extract_go_rust`-style suite against a
real C parse; naive-skip bug class asserted absent (a test that fails
if extraction silently returns nothing); a curl/mitlab-grade real C
file parses in conformance-style spot-check.

---

### [x] R13 · Languages #8–9: C++ and C# (M) — DONE 2026-08-30

*(Implementation summary: C++ — `_CPP_CLASS_TYPES` + body/name gate on
the walker (C's struct-invisibility stays pinned for `.c`),
`field_identifier`/`qualified_identifier` extensions to `_c_declarator_name`,
base_class_clause bases; C# — `_CSHARP_CLASS_TYPES`,
`_csharp_class_bases` (base_list first=extends/rest=implements,
documented), `invocation_expression`+`member_access_expression` in the
call pipeline; `.h` re-scoped to C headers with the mis-tree finding;
10 tests across both languages; counts 7→9 (README, landscape,
pyproject).)*

**State today (verified).** Same tables as R12; C++ and C# each reuse
R12's declarator path but diverge exactly where the roadmap says:
inheritance/impl resolution. Verified specifics to handle: C++
`class_specifier`/`struct_specifier` with `base_class_clause`
(virtual/visibility tokens inside), methods as
`field_declaration_list` entries with real `function_declarator`s
(tricky: names inside `pointer_declarator` for operators/
destructors); C# `class_declaration`/`interface_declaration` with
`base_list` (which carries contains both `extends`-and-`implements`
semantics in one clause — relation split by marker), `method_declaration`
with `name` field (friendlier, field-based).

**Plan.**
1. Two deps in one pass: `tree-sitter-cpp`, `tree-sitter-c-sharp` (pin
   versions; C# grammar package name is `tree-sitter-c-sharp` →
   module `tree_sitter_c_sharp`).
2. Reuse `_declarator_unwrap` from R12 for C++'s nested declarators; C#
   mostly rides the existing field-based name helper — assert that with
   a test, don't assume it.
3. Per-language inheritance: C++ `base_class_clause` → `extends`
   (access-specifier stripped); C# `base_list` → split
   first-identifier-`extends` / rest-`implements` (document the rule in
   the function's docstring, C#'s single-clause reality, verified
   against real parses — same verification standard as
   `_java_class_bases` at extract.py L642).
4. Update the three docs + pyproject + README count in the SAME PR as
   the code (no-stale-docs rule) and keep landscape's honest statement:
   this is 6 → 9, **not** 9 ≈ 21–40+; the gap sentence stays.

**Impact / ripples elsewhere.** Same set as R12 ×2 languages:
`tests/test_extract_cpp.py`, `tests/test_extract_csharp.py`;
`docs/adding-a-language.md` gains the "declarator languages" pattern
note; conformance surface unchanged (no new tools); benchmark docs
unaffected unless a C repo enters R9's set (note only).

**Verify (from roadmap).** R12's bar per language, plus: C++ multiple-
inheritance and C# interface-list parses assert exact base/relation
pairs; README/landscape/pyproject updated in the same pass.

---

### [x] R14 · PRD-hierarchy port to embedded SQLite (M) — DONE 2026-08-30

*(Implementation summary: `cie/embedded_hierarchy_repository.py` (same
protocol, shared validators, documented backend differences — single
HAS_CHILD direction, name-keyed unconditional REALIZED_BY);
`hierarchy_repo` param on ToolService + `hierarchy_tracking=False` /
`--no-hierarchy` opt-out with
`unavailable[HIERARCHY_STORE_NOT_CONFIGURED]`; three hierarchy tools
promoted (R1's playbook, incl. the WRITE_TOOLS trap); factory + cli
+ mcp wiring; 21 tests at B1's bar; conformance re-run: 135 tools,
100 verified / 26 graceful / 5 unavailable / 0 crashes; README#undefers
"still Neo4j only"; stale docstring fixed. push_hierarchy trip-wire
verified by the pinned invariant test firing exactly once when the
alias-set/WRITE_TOOLS swap was momentarily half-done.)*

**State today (verified).** `cie/hierarchy.py` holds the full
`HierarchyRepository` protocol (L151–188: `push_hierarchy`,
`get_children`, `get_lineage`, `get_hierarchy_node`, `get_project_tree`)
+ `Neo4jHierarchyRepository` (Cypher, APOC-free). The docstring claims
an in-memory fake at `tests/in_memory_hierarchy_repo.py` — **that file
does not exist** (verified `find`), a stale reference to fix here.
Embedded has *no* hierarchy story; `factory.build_tool_service_embedded`
doesn't thread one; `mcp_server --embedded` therefore can't serve
`push_hierarchy`/`get_children`/`get_lineage` even after R1's
promotion. CLI `hierarchy:*` commands route through `_open_hierarchy_repo`
(hardwired Neo4j) — R2's seam covers them once the repo exists.

**Plan.**
1. `cie/embedded_hierarchy_repository.py`: `SQLiteHierarchyRepository`
   implementing the protocol, backed by the same two-table style as
   `EmbeddedRepository` (a `hierarchy_nodes` table with
   id/parent_id/type/props JSON + `project` column; the
   `_bev2_rel_type_union()` shim's edge-type nuance collapses here —
   SQLite stores parent_id, but preserve the label-union semantics in
   `get_children`'s filtering so `get_project_tree` returns the same
   discover-profile views as `TYPE_TO_LABEL` maps, L60–80).
   Validation lives in `cie/hierarchy.py`'s shared helpers
   (`find_repeated_id`, `count_nodes`) — import, don't duplicate.
2. Wire: `build_tool_service_embedded` gains the hierarchy repo
   (respecting `--no-task-tracking`'s precedent for a
   `--no-hierarchy` fail-fast mode); mcp_server's `--embedded` path
   inherits it automatically via the factory; CLI `_open_hierarchy_repo`
   switches via R2's seam.
3. **Complete R1's hierarchy promotion here:** `push_hierarchy`,
   `get_children`, `get_lineage` become ToolService methods (routes'
   alias handlers → `_service_tool`), `push_hierarchy` lands in
   `WRITE_TOOLS`, `HTTP_WRITE_ALIASES` empties, invariant-sets updated —
   reusing R1's playbook exactly, including the WRITE_TOOLS trap.
4. Tests at the B1 depth: `tests/test_embedded_hierarchy_repository.py`
   mirroring `test_embedded_task_repository.py`'s 17-test bar (CRUD +
   lineage + project scoping + repeated-id rejection + empty-tree
   envelopes).
5. Docs drop the "Neo4j-only" caveat: README task/QA section,
   `docs/language-agnostic-design.md` backend matrix, CHANGELOG;
   fix `cie/hierarchy.py`'s stale fake-pointer docstring in the same
   PR.

**Impact / ripples elsewhere.** `mcp_server --embedded`'s help text
(task-tracking flag text); R1's invariant updates; conformance re-run
(surface grows by the 3 new ToolService methods — Count Contract);
roadmap cross-ref: this closes goal.md's own "last Neo4j-only feature"
note.

**Verify (from roadmap).** Hierarchy CRUD + lineage tests at the
17-test bar against SQLite; `push_hierarchy` over `--embedded` MCP
round-trips a real tree; docs carry no "Neo4j-only" hierarchy caveat
(grep-clean).

---

## P2 — adoption & launch mechanics (gated on R6)

### [x] R15 · `cie init` one-command onboarding (M) — DONE 2026-08-30

**State today (verified).** No `init` command exists; README's
quickstart (L85–107) ends with "add it to Claude Code / Cursor / Codex
the way you'd add any other local MCP server" — i.e., all-manual.
GitNexus's AGENTS.md/CLAUDE.md files and CodeGraph's installer are the
named exemplars (delta S2).

**Plan.**
1. `cie init [PATH]` (new command in `cie/cli.py`, after R2's seam):
   detect installed clients by config-file presence — Claude Code
   (`~/.claude.json` / project `.mcp.json`), Cursor
   (`~/.cursor/mcp.json`), Codex (`~/.codex/config.toml`) — `--client`
   override flag, `--list` printout; register the stdio server entry
   (`command: "cie-mcp"`, `args: [<abs path>, "--embedded"]`,
   `--policy readonly` **default** with `--policy` override, print the
   exact diff before writing, idempotent re-runs).
   Safety default note: readonly-until-chosen avoids surprising write
   powers in a client the user didn't intend to grant them — surface
   the choice, default safe (this is the fallback-differentiator d4
   story in product form).
2. Write context files: `<root>/AGENTS.md` + `CLAUDE.md` (append-or-
   create with a marked managed block, never clobber user content):
   what cie tools exist, when to use `callers` vs `grep`, the
   export-html hint, and the task/QA traceability one-liner — sourced
   from a template const in `cie/init_templates.py` so tests can pin
   it.
3. Demo: extend `scripts/record_demo.sh` pattern — a `record_init.sh`
   capturing `cie init` → client listing the tools (the demo cast
   helps, keep it ≤30s).

**Impact / ripples elsewhere.** README quickstart slims to `pip
install → cie init` with the manual path demoted to an appendix;
`docs/competitive-landscape.md` handgun the "no installer" honest-gap
row; CHANGELOG; CONTRIBUTING unchanged; watch W2 (policy-proxy) not
affected.

**Verify (from roadmap).** Fresh clone → `cie init` in a container/VM
with one real installed client → client lists cie tools with zero
manual config; captured for the demo cast; `--policy` default honored
by what the client lists.

---

### [x] R16 · `run`-tool isolation story made explicit (M) — DONE 2026-08-30

**State today (verified).** The honest core exists:
`cie/tools/runner.py` L3–8 states "v0 isolation is subprocess only —
cwd jail plus hard timeout", `routes.py` carries the "no container
isolation yet" comment, and the policy side is real (WRITE_TOOLS
gates `run`/`run_tests`; HTTP default-inspector refuses, verified this
session per goal.md). Missing: a written threat model, a per-surface
refusal test, and the optional container mode.

**Plan.**
1. Docs first: `docs/security.md` — the `run` boundary precisely:
   what the jail is (cwd via `allowed_root`, resolve-confined, hard
   timeout + process-group kill, bounded output — cite runner.py
   L96–122), what it is NOT (no fs sandbox beyond cwd for child
   processes that have their own permissions, no network restriction,
   no container), the threat model (trusted local user; untrusted
   HTTP/MCP callers get policy refusal, not jail security), and the
   escape hatch `CIE_RUN_ROOT` exists and what widening it means.
2. Optional container mode: a documented, *minimal* seam —
   `CIE_RUN_WRAPPER="docker run --rm -v {root}:{root} -w {root}"`-style
   prefix executed through the same runner (no new dependency, no
   docker SDK) + docs stating it's convenience, not enforcement.
3. The pinned test: extend `tests/test_http_policy.py` with a one-test
   loop asserting every networked surface (HTTP `POST /tools/run`,
   MCP under readonly policy, CLI server path) refuses `run` by
   default with `ToolNotPermitted` semantics — the machine-checkable
   half of the claim.

**Impact / ripples elsewhere.** README security footnote links the
doc; `runner.py` docstring cross-references; CHANGELOG; R11's harness
reuses the refusal assertions; mcpservers listing (R18) can safely
state the boundary.

**Verify (from roadmap).** `docs/security.md` states the threat model;
policy test pins read-only refusal of `run` on every surface;
container mode demonstrated once in a cast (optional, doc'd).

---

### [x] R17 · Conformance in CI (S) — DONE 2026-08-30

**State today (verified).** `.github/workflows/ci.yml` runs pytest on
py3.10–3.13 + an MCP stdio smoke step (initialize + tools/list) — good
floor, but conformance (`tool-test-lab/surface_conformance.py`) runs
only when a human remembers. Known blocker inside the harness itself:
L107 hardcodes `PYTHONPATH=/home/arun/Downloads/cie` (its README warns
to edit it).

**Plan.**
1. Fix the pin: derive from `Path(__file__).resolve().parents[1]` (repo
   root), keeping an env override for sandbox use — resolves the
   tool-test-lab README's ⚠ note.
2. Add a CI `conformance` job: `pip install -e ".[mcp,http]"`, index a
   committed micro-fixture repo (`tests/fixtures/sandbox/` — 5 files,
   deterministic content; matches the harness's `app.py`-guessing
   `arg_value` heuristics), run
   `python tool-test-lab/surface_conformance.py <sandbox>
   surface_results_ci.json`, then a small comparator:
   - **0 CRASH** always;
   - surface count == `ToolService` introspection count read in-process
     (never a magic constant forever — store an approved-counts file,
     `tool-test-lab/approved_surface.json`, updated *only* in PRs whose
     point was a surface change, so silent drift is red and deliberate
     drift is a reviewed diff);
   - every `unavailable-by-design` entry must carry an approved `reason`
     (ties into R5's registry).
   pytest should invoke this where feasible (a
   `tests/test_surface_conformance.py` marked slow, run in the same
   job) so the invariant is enforced by the test runner, not only yml.
3. Prove the red/green path once on a branch (intentionally break one
   tool → CI red → restore → green) and record the run links in
   goal.md.

**Impact / ripples elsewhere.** `tool-test-lab/README.md` (un-warn the
path pin, document the CI contract); `approved_surface.json` joins the
artifacts-of-record list; R3's count convention becomes CI-enforced —
after this lands, the no-stale-docs rule's tool-count half is
machine-checked.

**Verify (from roadmap).** Intentionally break one tool → CI red;
restore → green; invariant machine-checked from then on.

---

### [ ] R18 · Directory listings (S, human action — gated on explicit go-ahead) — drafts DONE 2026-08-31

*(Step 1 complete: `docs/directory-listings-drafts.md` holds the
mcpservers.org field set and the awesome-mcp-servers PR text, every
string sourced from the shipped README, R6 gate satisfied. Submission
awaits the owner's explicit go-ahead; README badge row stays un-added
until listings are live.)*

**State today (verified).** goal.md E3 names the two targets
(mcpservers.org/submit, awesome-mcp-servers PR) and the rule: external
action under this account needs explicit go-ahead, per goal.md's "needs
your go-ahead" framing. Draft complete 2026-08-31; README has no links
to add yet (correctly — no live listing to link).

**Plan.**
1. Draft (no submission): mcpservers.org entry fields (name, tagline
   from README L22's differentiator sentence, install command from
   quickstart, stdio transport note from R11 work, policy note);
   awesome-mcp-servers PR text (one line, section placement per repo
   convention, link to README + benchmark docs).
2. Wait for explicit go-ahead (the gate is the point), then submit both;
   capture PR URLs into goal.md E3.
3. After listing is live: README "Listed on" badge row + links back.

**Impact / ripples elsewhere.** README badge row only after live
(never link-to-404); R6 must have shipped first (C1 gate) since
listings point at the stable artifact.

**Verify (from roadmap).** Entries live; README links them; goal.md E3
checked by hand.

---

### [x] R19 · Repo trust signals + second-maintainer activation (S) — DONE (release page rides R6) 2026-08-31

*(Implementation: description + 10 topics applied via `gh repo edit`
2026-08-31; three `good-first-issue` issues created — CLI-tests GFI →
#17, failing_context ground truth → #18, urllib3 reproducibility → #19;
README label link landed in 5b527c4; drafts + stamps in
`docs/repo-trust-signals.md`. The release page is R6 step 3's own
checklist item (notes rendered from CHANGELOG at cut time). R6's PyPI
upload is blocked by a real finding: pypi `cie` is an unrelated
project.)*

**State today (verified).** CONTRIBUTING.md has a real
second-maintainer path (README L93 links "Becoming a second maintainer"
— the C4 work). Missing: GitHub repo description/topics, a rendered
release-notes page, labeled starter issues.

**Plan.**
1. Repo settings: description (differentiator one-liner, stable-version
   note), topics (`mcp`, `code-graph`, `tree-sitter`, `static-
   analysis`, `llm-tools`, `neo4j`, `sqlite`).
2. Release page: rendered from CHANGELOG 0.1.0 section (R6's notes
   body) — do once at R6, keep in step per release (R6 checklist
   notes it; this item adds the *issues* half).
3. 2–3 `good-first-issue` issues chosen against CONTRIBUTING's named
   safe-entry modules (pick from R12/R13's test-suite adjacencies or
   R17's approved-surface workflow — genuinely small, well-scoped,
   verifiable items), each with acceptance criteria + pointer to
   CONTRIBUTING's review bar.
4. README: link the issues from the contributing section.

**Impact / ripples elsewhere.** CONTRIBUTING may need a one-line
"current starter issues" pointer (dated — or link the label instead of
hardcoding numbers, so it never rots).

**Verify (from roadmap).** A newcomer reaches a labeled starter issue
from README in ≤2 clicks; C4's path verified as true by walking it.

---

### [ ] R20 · Launch post, drafted but gated (S + timing) — draft DONE 2026-08-31

*(Steps 1–2 complete: `docs/launch-post-draft.md` holds the Show HN +
r/LocalLLaMA variants and the claim-by-claim table — 14 rows, every one
footnoted to its measured source (file + section + date), losses kept
in the post body per critique #10 (the 3-of-6 / 28-of-40 recall gaps,
per-question MRR misses, vendor-claims labeling rule). Publish gates
§D: 1–3 satisfied; 4 (re-run the measurable rows on the publishing
commit), 5 (explicit go-ahead), 6 (HN timing) remain. Nothing has been
published from here.)*

**State today (verified).** All raw material exists and is measured:
demo (recorded), benchmarks (2 docs + R9's third), tool-selection
accuracy (14/14 runs), surface conformance numbers, the honest-loss
sections. The declined critique (#10, session 3) and the claim-audit
demand come from goal.md.

**Plan.**
1. `docs/launch-post-draft.md`: Show HN + r/LocalLLaMA variants, both
   built from R8/R9 artifacts (the HTML export screenshot and the
   reproducible benchmark script are the post's spine — claims the
   reader can reproduce in 3 commands).
2. The claim-by-claim table: every sentence that asserts something
   footnoted to its measured source (file + section + date): benchmarks
   docs, `surface_results.json`, `docs/tool-selection-accuracy.md`,
   competitor claims labeled vendor-claims only. **Losses stay in the
   post** (the 3-of-6 miss, the tie in Task 1, R9's reported misses) —
   that's the floor of the honesty stance, non-negotiable per
   session-3 critique #10.
3. Publish gate: R6 shipped (not merely drafted), R9's numbers public,
   a re-read for rate-limits/timing (Show HN best practice), and the
   user's explicit go-ahead. Draft everything; publish nothing from
   here.

**Impact / ripples elsewhere.** If R9/R10 moved numbers, the post cites
the newest dated values; the audit table double-checks R3's counts —
it is the last walk of the Count Contract before an audience sees it.

**Verify (from roadmap).** Claim-by-claim table in the draft, each row
footnoted to its measured source; nothing published from vibes.

---

## P3 — the next 25 (R21–R45): defend, widen, mature

> Added 2026-08-31 (planning pass 2, rewritten at R1–R20 depth in pass 2b),
> from `docs/competitive-delta-2026-08-30.md` (WATCH W1–W4 promoted per
> the re-sequencing rule — the owner's next-tranche direction is the
> trigger) and `docs/competitive-landscape.md` (honest gaps). `roadmap.md`
> P3 holds the why/scope; this section holds the executable plans.
>
> Baseline when planned (verified on this tree, suite 292/292): surface
> 135 tools — 101 verified / 25 graceful / 5 unavailable (4
> `OPTIONAL_BACKEND_MISSING:core` + 1 `HOST_PLUGIN_MISSING`) / 4
> backend-down / 0 crashes; 0.1.1 shipped on GitHub as `cie-mcp` (PyPI
> upload pending credentials); WATCH W1–W4 promoted: W1→R24/R25,
> W2→R27, W3→R26, W4→R28.
>
> The Count Contract carries over: every item that moves the surface or
> a conformance bucket re-measures and updates README's tool-count
> labels, `docs/tool-selection-accuracy.md`, `docs/language-agnostic-
> design.md`, CHANGELOG (dated entry), and re-runs `tool-test-lab/
> surface_conformance.py` committing fresh `surface_results.json` +
> `approved_surface.json` when the tool count moves.

---

### P3a — stable-story mechanics (small, unblocks adoption)

### [ ] R21 · Ship `cie-mcp` to PyPI (S) — closes R6's step 5

**State today (verified).** `dist/cie_mcp-0.1.1{.tar.gz,.whl}` built,
`twine check` PASSED, `METADATA` reads `Name: cie-mcp / Version: 0.1.1`,
entry points `cie = cie.cli:main` + `cie-mcp = cie.mcp_server:main`;
clean-venv rehearsal from the wheel passed (index 858 nodes/1,822 edges
on the pinned psf/requests clone; stdio handshake 85 read-only tools).
Blocked ONLY on credentials: no `~/.pypirc`, no `~/.config/twine`, no
repo secrets (`gh secret list` empty), no publish job in
`.github/workflows/` (only `ci.yml`). Tag `v0.1.1` + GitHub release live
on `234605a`. `pypi.org/pypi/cie-mcp` returns 404 (name free, verified
2026-08-31).

**Plan.**
1. Owner picks ONE path: (a) hand-run — `TWINE_USERNAME=__token__
   TWINE_PASSWORD=<token> python -m twine upload dist/*` from the repo
   root (twine 7.0.0 is installed in `.venv`); or (b) preferred,
   automated — add `.github/workflows/publish.yml`: trigger
   `on: push: tags: ['v*']`, steps = checkout → `uv build` →
   `twine check dist/*` → `pypa/gh-action-pypi-publish` with
   `password: ${{ secrets.PYPI_API_TOKEN }}`; the owner adds the token
   as the `PYPI_API_TOKEN` secret once (trusted publishing instead
   requires the PyPI-side pending-publisher entry for `cie-mcp` —
   either is fine, don't mix).
2. Re-push `v0.1.1` (delete + re-create the tag at `234605a`) to fire
   the workflow on the already-released version, or wait for the next
   version — decide with the owner; the release notes already say
   "from PyPI once the 0.1.1 upload lands".
3. Post-upload: flip README's install block — PyPI line first
   (`pip install "cie-mcp[mcp]"`), GitHub install demoted to the
   alternative, the dated name-note stays (it's still true and useful).

**Impact / ripples elsewhere.**
- R18's `docs/directory-listings-drafts.md` install field (already
  written as the PyPI string — becomes true) and R20's draft install
  line + audit-table row 11 (same).
- `goal.md` C1 follow-up note says "upload runs with the maintainer's
  credentials" — flip to done with the date once live.
- No code, no surface change; CHANGELOG gets a dated line under
  [Unreleased] noting the artifact is live (not a version bump —
  0.1.1 is the artifact being uploaded).

**Verify.** Fresh `uv venv`, `pip install "cie-mcp[mcp]"` **from PyPI**
(not the local wheel), then the exact R6 rehearsal: `cie index` the
pinned requests clone → 858/1,822; `cie callers close` → 3; stdio MCP
handshake lists 85 read-only tools and refuses write tools. `pip show
cie-mcp` metadata correct; uninstall/reinstall clean.

### [ ] R22 · CI hardening: OS matrix + Dependabot merge (S)

**State today (verified).** `.github/workflows/ci.yml` runs
`ubuntu-latest` only (L17) with a `python-version` matrix
`["3.10","3.11","3.12","3.13"]` (L21), plus the MCP stdio smoke step
and the R17 conformance job. Two Dependabot PRs are open and CI-green
(2026-08-30): click `>=8.1.0 → >=8.5.0` and tree-sitter-c
`>=0.23.0 → >=0.24.2` — `tree-sitter-c` is a **core** dependency
(pyproject L33), so the floor-raise touches `pyproject.toml` and needs
the C extraction suite re-run, not just a green merge.

**Plan.**
1. Merge the tree-sitter-c PR first: rebase, run
   `tests/test_extract_c.py` + full suite locally at the raised floor,
   confirm the pinned C fixtures still parse identically (grammar
   minor-version drift is the risk; the R12 mirror-bug tests are the
   tripwire). Then the click PR (CLI deprecation warnings in
   `tests/test_cli.py` — the `isolated_filesystem` DeprecationWarning
   seen in the suite output — may actually improve).
2. Add `strategy: matrix: os: [ubuntu-latest, macos-latest]` to the
   pytest job (`runs-on: ${{ matrix.os }}`); add `windows-latest` as a
   separate allow-failure job initially (uv/tree-sitter wheel
   availability on win varies) with `continue-on-error: true` and a
   TODO to graduate it.
3. Keep the conformance job Linux-only (it shells `cie-mcp` and the
   approved-surface comparator; no value in matrixing it yet).

**Impact / ripples elsewhere.** pyproject floors move in the
tree-sitter-c merge (CHANGELOG line under [Unreleased]); the R6/R21
clean-venv rehearsal docs stay valid (they install latest anyway).

**Verify.** Matrix green on a pushed commit (all OS × 3.10–3.13 legs
+ conformance job); suite 292/292 locally at the raised floors; no
behavior change in C fixtures.

### [ ] R23 · Nightly benchmark CI (S)

**State today (verified).** Benchmarks are session-run artifacts:
`scripts/benchmark.sh` (signature L18: `URL COMMIT SUBDIR GLOB CLASS
AMB BIG` — clones to a mktemp dir) drives `scripts/benchmark_tasks.py`;
`scripts/benchmark_semantic.py` refuses to run without `CIE_EMBED_DSN`
+ key (`_require_env`, L~57). Published docs:
`docs/benchmarks-requests.md`, `docs/benchmarks-urllib3.md` (+
`docs/benchmarks-semantic-*.json` raw artifacts). `ci.yml` has **no**
`schedule:` trigger; nothing re-runs the numbers between releases, so
the R3 count-drift class has a performance cousin nobody watches.

**Plan.**
1. New job in `ci.yml`: `on: schedule: cron "0 3 * * *"` (+ 
   `workflow_dispatch` for manual runs) → checkout → `uv venv` →
   `pip install -e ".[mcp]"` → run `scripts/benchmark.sh` twice (the
   two pinned corpora, exactly the doc's env vars) → upload both JSONs
   as workflow artifacts (retention ~90d).
2. A tiny comparator step: against the committed
   `tool-test-lab/benchmark_baseline.json` (new, committed in this
   PR), alert when any headline metric drifts >20% (resolved-callers
   ratio, skeleton ratio, per-task payload chars) → open an issue
   automatically (gh CLI is authed in Actions with default token).
3. Semantic variant: same job, guarded on
   `secrets.CIE_EMBED_DSN != ''` — skipped with an honest log line
   otherwise (never a silent fallback to lexical-only, per R10's
   rule).
4. Update the baseline file in the same PR whenever a deliberate
   change moves the numbers (the R17 approved-surface discipline,
   applied to performance).

**Impact / ripples elsewhere.** GitHub Actions cron is best-effort —
document that in the doc line (not a monitoring SLA). Drift issues
land in the repo R19's `good-first-issue` discipline protects from
noise — label them `benchmark-drift`, never `good first issue`.

**Verify.** Two consecutive nightly runs produce comparable artifacts;
once, on a branch, deliberately break a tool (R17's red/green proof
pattern) and confirm the drift issue fires; then restore.

---

### P3b — differentiator defense (WATCH promotions)

### [ ] R24 · `cie impact` — PR test-impact (M) — PROMOTED W1

**State today (verified).** Every building block exists and is
conformance-verified, none composed: `ToolService.sync_load_commit`
(L1623) loads a git commit's tree into the speculative graph,
`sync_ast_delta` (L1572) diffs one file, `callers` (L644) /
`callees` (L681) / `affected_by` (L791, `max_depth=3`) give blast
radius, `test_map` (L979) is the reverse-TESTS lookup, and TESTS
edges are real index-time artifacts — `cie/cli.py` L681/L705 calls
`cie.testlink.resolve_test_edges(per_file, call_edges)` (DM-14: test
symbol → implementation). `failing_context` (L733) already maps a
*failed* test to context. No impact tool on the 135-tool surface
(verified by introspection); CodeGraph's per-PR platform announcement
is the WATCH W1 attack this answers [vendor claim, pre-launch].

**Plan.**
1. `ToolService.impact(diff, project)` — input: either a commit hash
   (route through `sync_load_commit`'s changed-file set) or an
   explicit list of paths (route through `sync_ast_delta` per file).
   Output envelope: `changed_symbols` (ids+labels+files), `blast`
   (closure over `calls`/`contains` from the changed set, capped,
   depth honored), `tests` (the ranked must-run set: TESTS edges into
   the blast + task links via `traceability_chain`'s join, each row
   carrying `reason: {tests_edge|task_link|distance}`), and
   `invariants` (contracts touching changed symbols — reuse the
   `HAS_CONTRACT` join `cie/invariants.py` L94 already documents).
   Rank = distance first, then observed failure history once R26
   lands (design the field now, populate when R26 does).
2. Surface plumbing, all three fronts in one commit (the parity
   contract `tests/test_tool_surface_invariants.py` pins): `cie
   impact` CLI command (follow `cie path` at `cli.py` L1006 — click
   args + `--json`), HTTP route via `TOOLS["impact"] =
   _service_tool("impact")` (routes.py L528 pattern), MCP comes free
   by introspection. **Read-only** — no `WRITE_TOOLS` entry (the
   R1 trap in reverse: verify `inspector` policy lists it).
3. Count Contract: surface 135 → **136**; update
   `tool-test-lab/approved_surface.json` `tool_count`, README labels,
   CHANGELOG dated entry, fresh `surface_results.json` committed.

**Impact / ripples elsewhere.**
- `docs/competitive-landscape.md`'s W1-adjacent prose gains the
  measured "we ship PR test-impact" row (with the vendor-claim label
  kept on CodeGraph's announcement).
- R25 composes this; R44's `pr` gate profile calls it; R26 feeds its
  ranking — design the envelope once, here.
- `sync_*` tools carry the two-graph validation ("requires a non-empty
  project" — conformance sandbox hits this): `impact` over an
  unbound service must degrade to that same honest validation
  envelope, not a silent empty.

**Verify.** Ground-truth fixture in `tests/test_graph_semantics_
ground_truth.py`'s style: a repo with a known diff → known changed
symbols → exact expected test set (known-by-inspection), embedded +
Neo4j-double; live check on the pinned psf/requests clone (a real
commit touching `sessions.py` must surface the right tests);
conformance re-run with the new tool verified, 0 crashes.

### [ ] R25 · PR review pack (M) — PROMOTED W1

**State today (verified).** The composer exists in three pieces:
`cie/export_html.py` already builds a static zero-network artifact
from ToolService envelopes (`_gather` L37, renderers L123–L189:
overview/chains/tasks/orphans/files, TESTS filter L74), R24 will own
the impact slice, and `sync_load_commit` (L1623) + `semantic_diff`
(L2106) + `traceability_orphans` + `drift_detect_run` are the
content sources. No diff-scoped artifact exists today — export-html
is whole-project.

**Plan.**
1. `cie review-pack [PATH] --commit <hash> [--base <ref>] --out
   FILE`: one pass over the changed set → sections: changed symbols
   (speculative-vs-canonical via `sync_load_commit` then per-file
   `sync_ast_delta`), impact block (R24's envelope, rendered),
   contracts/invariants touched, new orphans created by the change,
   and the honest-miss footer (R8's pattern).
2. Implementation discipline: compose existing ToolService envelopes
   exactly as `export_html._gather` does — no new graph access paths,
   no server, `file://`-openable, zero external references (the R8
   test bar: `tests/test_export_html.py`'s no-network assertions
   extend to the new artifact).
3. Script + screenshot via the `record_export_html.sh` pattern
   (clone-pinned-commit → generate → headless capture), committed.

**Impact / ripples elsewhere.** README gains a "review packs" line in
the traceability section; `docs/security.md` unchanged (artifact is
static read-only output — state that in its docstring); R20's post
gains a second reproducible artifact to point at.

**Verify.** Pack for a pinned real commit matches a hand-derived
expectation list (changed symbols, must-run tests); zero-external-ref
assertion in tests; script reproduces the committed screenshot.

### [ ] R26 · Runtime evidence overlay (M) — PROMOTED W3 (test-run level)

**State today (verified).** The ingest surface half-exists:
`record_test_result` (L2573) writes a TestExecution's outcome;
TestExecution nodes are written via the §13 analysis-node pattern —
`replace_analysis_nodes("TestExecution", nodes, edges, project=)`
(L2539), the same seam `CallResolutionStat` uses. `run_tests` already
collects per-test latency from pytest's own `--junitxml` (TE-08,
`cie.apm.collect_apm_from_pytest`) — proving junitxml parsing exists
in-tree. `record_coverage_snapshot` (L4483) persists measured
coverage. What's missing: a junitxml → TestExecution/results importer
that maps failures onto TESTS-edge code nodes, and OBSERVED-* edge
kinds on the graph. eBPF-level tracing stays watch-listed (W3's own
note) — this is test-run evidence only.

**Plan.**
1. `cie ingest-results <junitxml> [--suite name]` (CLI + optional
   ToolService method `ingest_results`, read-write → `WRITE_TOOLS` +
   policy tests per R1's playbook): parse failures/errors/skips+times,
   create/refresh TestExecution analysis nodes, then resolve each
   failed test name to its TESTS-edge target symbols (the reverse of
   `test_map` L979) and write `OBSERVED_FAILED` /
   `OBSERVED_FLAKY` (failed-then-passed) analysis edges, plus
   `OBSERVED_COVERED` from coverage snapshots.
2. R24's ranking field (`failure_history`) starts being populated:
   observed-failure count per symbol, recency-weighted; envelopes that
   surface it say so (additive field, no shape break).
3. Name-resolution discipline (the honest part): a junitxml test
   name that matches no TESTS edge must be **reported as unresolved
   in the output** (`unresolved: [names]`), never silently dropped —
   the R7 "make the gap visible" rule, applied again.

**Impact / ripples elsewhere.** Conformance: one new write tool
(`ingest_results`) → surface 136→137 if R24 landed first (Count
Contract in the same commit); `WRITE_TOOLS` + invariant tests;
`test_unavailable_reasons` untouched (nothing 503s here). Edge kinds
are additive to `INVERSE_RELATIONS` (`data_model.py` — add
`OBSERVED_FAILED: "observed_failure_of"` etc.).

**Verify.** Replay the repo's own real junitxml into a fixture graph;
assert TestExecution nodes + OBSERVED edges exactly match
known-by-inspection truth; unresolved names appear in the output
list; R24's `impact` ranking shifts when a failed test overlaps the
blast radius (integration test R24+R26 together).

### [ ] R27 · Policy profiles 2.0 (M) — PROMOTED W2

**State today (verified).** Policy surface is real and server-side on
every transport: `cie/tool_policy.py` — `AgentType` enum (L40–51:
forge/miner/inspector/orchestrator…), `permits(tool_name)` (L110),
hand-curated `WRITE_TOOLS` (the L18 comment states the maintenance
rule), `--policy` choice wired in both `cie-mcp`'s arg parser (mcp_
server.py) and `cie init`'s registered entry (`init.py` L45–46 —
readonly by default, operator opts into `--policy full`). Missing vs
SocratiCode's proxy slice [vendor claim]: no policy FILES (patterns,
not enums), no per-client binding beyond init's one-shot write, no
refusal audit.

**Plan.**
1. Profile file format — **TOML via stdlib `tomllib`** (no new dep;
   same choice pyproject made for `<3.11`): `[include] patterns =
   ["callers", "test_*"]`, `[exclude] patterns = ["run", "push_*"]`,
   optional `[settings] audit = true`. Loader validates eagerly —
   an unknown tool-pattern class fails fast with the offending line,
   never a silent allow (the `_authorize_http_tool` failure mode to
   avoid).
2. `cie-mcp --policy-file <path>` and `cie init --policy <path>`
   (profile reference written into the client entry's args); runtime
   = base `--policy` enum ∩ file rules. **Invariant (the R1-trap
   pattern one level up):** a profile may only *remove* from its
   base enum, never add — pinned by a test matrix (profile × enum).
3. Refusal audit log: append-only JSONL beside the DB
   (`.cie/policy_audit.jsonl`), size-capped (rotate at N MB, keep 1
   prior), opt-in via profile settings; records
   `{ts, surface, tool, policy, outcome}` for **refusals only**
   (never payloads — they can contain code/secrets).

**Impact / ripples elsewhere.** `tests/test_http_policy.py` gains
profile cases; `tests/test_tool_surface_invariants.py` policy-set
tests must keep passing byte-for-byte for the enum path (profiles are
additive); `docs/security.md` gains the profile + audit section
(extending R16's threat model, same doc).

**Verify.** Per-surface profile tests (stdio dogfood harness + HTTP);
rotation test; the ∩-only-narrows invariant test; existing suites
green unchanged.

### [ ] R28 · Multi-repo workspaces (L) — PROMOTED W4, still gated on a real user

**State today (verified).** Every store is single-project-scoped: the
SQLite tables carry a `project` column (`embedded_repository.py`
schema, tasks/hierarchy same), `factory.get_engine(project)` (L52)
and `build_tool_service_embedded` (L168) bind one project per
service, and sync tools reject the unbound default ("the two-graph
model requires a non-empty project" — seen live in conformance).
Node ids are file-relative (`{file}::{name}@{line}`), so two roots
genuinely collide. The watch-list trigger (a real multi-repo user
asking) has NOT fired — keep it as the entry gate, per the
re-sequencing rule; this item is planned so the trigger has a
ready answer, not started.

**Plan (when triggered).** 1) `cie workspace add <path> --alias`,
`cie workspace list` (a small registry file in `.cie/` or `~/.config/
cie/` — decide then). 2) Id qualification: prefix nodes with the
alias at load (`{alias}::{file}::{name}@{line}`), keep per-repo DBs
(no megadb). 3) Workspace-level composables only (impact across
repos, hybrid_search across aliases) — per-repo tools keep their
shape with the alias in `source_file`. 4) Export/review-pack pick up
the alias column.

**Impact / ripples elsewhere.** Big: extraction ids, every
`source_file` consumer, export_html, R24/R25 compositions, `cie
init` (one entry per project or a workspace arg). This is why it
stays gated — it re-shapes identity.

**Verify.** Two-repo fixture with colliding `app.py::alpha@1` in both
roots: qualified ids distinct, per-repo queries unchanged, workspace
query returns both correctly labeled; no cross-root edge leakage
(absent-by-construction test).

**Addendum (2026-08-31, verified live — owner asked "shouldn't it
support multi-project?").** Today's multi-project reality, measured:
(1) **named instances in one client config work TODAY, embedded, no
new code** — `cie-web` + `cie-api` registered in one Claude Code
config, `claude mcp list` showed both `✔ Connected`, and a single
`claude -p` session calling `mcp__cie-web__search_symbol` and
`mcp__cie-api__search_symbol` returned the correct per-project files
(`/tmp/cie-multi/{web/app.py,api/svc.py}` — clients namespace tools
by server name, so instances coexist); (2) Claude Code project-scope
is per-project by construction (each project's `.mcp.json` from its
own `cie init`); (3) Neo4j mode namespaces many projects in one graph
first-class (`--project`; node `project` property + query filters,
`neo4j_repository.py` L451/L745/L270). **Found micro-gap:** `cie init`
registers the FIXED name `"cie"` (`init.py` L36 `_CIE_SERVER_NAME`),
so a USER-scope config (`~/.cursor/mcp.json`) can hold only one cie
entry — a second project's init skips "already registered". Cheap
fix when wanted: `--name cie-<alias>` (no identity re-shape). The
tricky parts the owner intuited are real and stay in this item:
cross-root id collisions (`{file}::{name}@{line}`), inter-repo
call/import edges (absent-by-construction today — no cross-root edge
leakage to preserve), hybrid ranking across aliases, Count Contract
per-project vs aggregate, per-project policy. The gate stands: the
owner's question is an interest signal, not yet a real workspace —
promote when one exists (a concrete repo pair + what they want to
ask across them).

---

### P3c — semantic layer completion

### [ ] R29 · First-party chat fallback → `qa` fully standalone (M)

**State today (verified).** R10 shipped exactly the embed half of
this: `cie/embed.py`'s tier-3 client (`_env_fallback_config` L65,
`_post_embeddings` L88, `_openai_compatible_embed_texts` L127,
`supports_embeddings` L144; dispatch tiers at L175/L194 — host
`core.llm` › registered override › env client › raise; gated on
explicit `CIE_EMBED_DSN` + key, the no-accidental-network rule).
Chat remains host-only at exactly five lazy call sites — `contracts.py`
L59, `state_machine.py` L56, `community_detect.py` L174 (each
`from core.llm import LlmAgent, Prompt, ask` inside the one function
that asks) and `graphrag.py` L117/L181/L440 (qa's gate + rerank +
answer step). The registry that pins the resulting bucket:
`tests/test_unavailable_reasons.py::EXPECTED_REASONS` (4 ×
`OPTIONAL_BACKEND_MISSING:core` + decompose_page). Conformance
bucket today: 101/25/5/4, 0 crashes.

**Plan.**
1. `cie/llm_compat.py`, mirroring embed's discipline: an
   OpenAI-compatible **chat** client (stdlib urllib; same env-gate
   shape — new vars `CIE_LLM_DSN` + `CIE_LLM_API_KEY`/`NVIDIA_API_KEY`
   + `CIE_LLM_MODEL`), plus the minimal `Prompt`/`ask` shim: builds
   system+user messages from the existing `system_prompt()`/`render()`
   methods, requests JSON-mode output, parses into the caller's
   pydantic output type, raises the same failure classes callers
   already catch. No agent registry needed — `ask(prompt)` returns
   `output_type(**json)`.
2. Dispatch at the five sites: `from core.llm import ...` stays first;
   on ImportError, fall through to `cie.llm_compat.ask` (which raises
   the honest RuntimeError when env isn't set — feeding the existing
   `_guard` → `unavailable[reason]` path unchanged). R5's lazy-import
   discipline preserved: the fallback import lives inside the same
   functions.
3. Registry + conformance in the SAME commit (the trap): with env
   set, `qa`/`contracts_run`/`state_machine_run`/`community_
   summarize_run` leave `unavailable-by-design`; without env they
   stay exactly as today. `EXPECTED_REASONS` gains an
   env-conditional note; conformance runs once env-less (bucket
   unchanged: 5) and once env-set (bucket → 1: decompose_page) — both
   artifacts committed, the second labeled with the env used.
4. Tests: `tests/test_llm_compat.py` — mocked transport (monkeypatch
   the same `_post_json` seam style as `test_embed_fallback.py`):
   request shape (system/user, JSON mode), output-type parsing,
   malformed-JSON failure, env-gate rules (bare key ≠ network), tier
   order (host override wins).

**Impact / ripples elsewhere.**
- Surface count unchanged (no new tools) but the conformance
  narrative changes — README's "~5 tools require the host LLM
  environment" footnote flips to "optional: env-gated first-party
  chat, 1 plugin-gated tool remains"; landscape's semantic-maturity
  paragraph gains a sentence; CHANGELOG dated entry.
- R30's A/B gets its LLM leg; R31's QA-task slice unblocks; R45
  finishes the story.
- No pyproject change (stdlib client; no openai dep — same choice
  R10 made for embed).

**Verify.** Mocked-transport suite green; live run with env set
(NIM chat model — `nvidia/…` pick at implementation, mirror R10's
probe-first step) answering a question over the indexed requests
clone with citations unchanged in shape; env-less conformance byte-
comparable to today's; the two artifacts committed with their env
labels.

### [ ] R30 · No-LLM rerank (S/M)

**State today (verified).** `graphrag.rerank` is an LLM ask with
degrade-to-hybrid-order on failure; `qa(use_reranking=True)` is the
seam. The retrieval math it would replace is public:
`in_memory_repository._HYBRID_*_WEIGHT` (L695–697: lexical .45 /
dense .45 / graph .10) and `_hybrid_normalized`. The labeled set for
scoring exists: 16 questions in `scripts/benchmark_semantic.py` with
per-question recall/MRR JSON committed.

**Plan.**
1. Structural reranker in `graphrag` (no LLM): rescore hybrid's top
   candidates by term coverage against the question, dense score,
   graph degree, TESTS-linkage boost, and same-file proximity —
   deterministic, seed-free by construction (sort with stable keys).
2. Wire: `qa` uses LLM rerank when available (R29), structural
   otherwise, none when `use_reranking=False`; the envelope records
   which ran (`rerank: "llm"|"structural"|"none"` — additive field,
   same disclosure rule as R7's provenance).
3. A/B/C on the labeled set: none vs structural vs LLM — publish the
   per-corpus MRR table with misses (the honesty bar), not a
   single-winner claim.

**Impact / ripples elsewhere.** `benchmark_semantic.py` gains the
third condition + the rerank-mode column; competitor-benchmarks doc
semantic section gets the A/B table (dated re-run note); no surface
change (field only).

**Verify.** Determinism test (same input → byte-identical order);
A/B table committed with both corpora; `qa` envelope shape otherwise
unchanged (existing tests green).

### [ ] R31 · Benchmark corpus #4 + tokenizer-pinned counts (M)

**State today (verified).** Corpora: psf/requests `5460f467`,
urllib3 `85a8a9cf` (+ the forge-internal one for the 08-28 doc);
`scripts/benchmark.sh` takes `URL COMMIT SUBDIR GLOB CLASS AMB BIG`
(L18) and clones into mktemp. Token counts are chars//4 everywhere
(`graphrag.py` `_CHARS_PER_TOKEN` L54 — AI-06's documented
approximation); no tokenizer dependency exists (pyproject verified).
C is parseable since R12/R13 but no C corpus has been measured.
Candidates on the delta's own criteria (ambiguous-name property,
≥10k LOC, famous): libuv, sqlite amalgamation, redis.

**Plan.**
1. Pick by dry-run probe (R9's own method — grep the candidate for
   ambiguous call targets first), then run `benchmark.sh` against it
   — one index serves this and R40.
2. Tokenizer pin: add `tiktoken` under a new **benchmark-only extra**
   (`[project.optional-dependencies] bench`), NOT a runtime dep;
   extend `benchmark_tasks.py` + `benchmark_semantic.py` to emit
   chars AND tokens per task; re-publish every benchmark doc table
   with both columns + a dated re-run note (numbers that move, move
   visibly).
3. The explicit vendor-comparison paragraph: our measured tokens vs
   codebase-memory-mcp's "120× fewer tokens" — labeled
   **vendor-claim vs measured**, no implied equivalence (R9's
   standing rule, restated in the doc).

**Impact / ripples elsewhere.** All four benchmark docs regenerate
(script-driven, pasted with run date + commit); README's benchmark
paragraph gains the 4th-corpus link; landscape's dataset-count
sentences update; CHANGELOG.

**Verify.** Fresh-clone reproduction from the script alone (GFI-3's
bar — which doubles as a live test of that issue's premise); tables
carry both metrics; no prose token-number without a table row.

---

### P3d — language breadth

### [ ] R32 · Language #10: Kotlin (M)

**State today (verified).** Nine languages in `_LANG_LOADERS`
(`extract.py` L72–107: py/js(x)/ts(x)/java/go/rust/c/cpp/cs), with
the per-language tables at L111–138 (`_CLASS_TYPES`, `_CPP/_CSHARP_
CLASS_TYPES`, `_FUNCTION_TYPES`, `_PARAM_TYPES`) and `_CALL_TYPES`
L244; `_docstring` (L520) takes a per-language branch; pyproject
tree-sitter pins at L26–35. The R12/R13 pattern is the template:
grammar dep + tables + name-path handling + a real-parse guard suite
(`tests/test_extract_c.py`, `tests/test_extract_csharp.py` — the
"naive-skip bug class" test included) + the counts sweep in the SAME
PR. Kotlin's known traps vs the existing paths: `fun` uses a `name`
field (C#-like, friendly — assert it, don't assume), receiver
functions (`fun String.foo()`) put the receiver in
`receiver_type` (map to an `extends`-shaped class edge or a
documented non-goal — decide in-PR and pin either way), companion
objects, and `object` declarations as class-shaped.

**Plan.**
1. pyproject: `tree-sitter-kotlin` pin; `_LANG_LOADERS` `".kt"/".kts"`
   entries; `_FUNCTION_TYPES` `function_declaration`; `_CLASS_TYPES`
   `class_declaration` + `object_declaration`; `_CALL_TYPES`
   `call_expression` with `simple_identifier`/`navigation_expression`
   receiver handling (mirror C#'s member-access work from R13).
2. Receiver functions: v1 scope = document-and-skip the
   `receiver_type` edge (Go/Rust's documented-gap precedent) OR map
   it — decide on a real parse, pin the decision in
   `docs/adding-a-language.md`'s "declarator languages" note either
   way (never silent).
3. `tests/test_extract_kotlin.py` at the R12 bar: exact-set
   positive assertions (classes, methods, top-level funs, params),
   the non-empty anti-naive-skip guard, a docstring/comment attach
   test with the honest-None case.
4. Counts sweep same-PR: README language list, landscape table +
  §7 sentence, `docs/language-agnostic-design.md` coverage table,
   pyproject, CHANGELOG (9 → 10).

**Verify.** Suite at the R12 bar green; a real Kotlin file (e.g.
from the grammar's own repo or a well-known OSS Kotlin file — pinned
commit) parses in a conformance-style spot-check; counts sweep clean
by grep.

### [ ] R33 · Languages #11–12: PHP + Ruby (M)

**State today (verified).** Same tables as R32; PHP's shapes are
field-friendly (`method_declaration` carries `name`; `object_
creation_expression`/`method_call_expression` with receiver via
`->`/`::`), Ruby's are name-based (`def` with a text body, `call`
nodes, `def self.x` receivers) — neither has declarator chains
(the C trap), which is why one pass carries both.

**Plan.**
1. `tree-sitter-php` + `tree-sitter-ruby` pins; loaders (`.php`,
   `.rb`, plus `.phtml`? — decide: no, document why); PHP: class/
   interface/trait + method + `->`/`::` receiver resolution to
   extends/calls edges; Ruby: `def`/`defs` (singleton defs →
   class-method), `call` sites, heredoc-safe signature capture
   (`_PARAM_TYPES` has no Ruby analog — signature from the def
   params node, documented approximation like Go's).
2. Two suites at the R12 bar (one per language, exact-set + guard +
   receiver tests); documented-gap sections per language in
   `docs/adding-a-language.md` (PHP: no trait-resolution edges v1;
   Ruby: no block semantics v1 — both pinned as honest non-goals).
3. Counts sweep 10 → 12 in the same PR (same file list as R32).

**Verify.** Both suites green; real-parse spot-checks (pinned files
from php-src and a famous Ruby gem at pinned commits); counts sweep
clean.

### [ ] R34 · Go/Rust import edges + docstrings (M) — closes a named gap

**State today (verified).** The gap is documented twice, by design:
`cie/extract.py`'s module header (L14 area: "NOT cover, honestly…
import-edge extraction") names it, and landscape §7 repeats it
("no import-edge extraction, no docstring extraction" for Go/Rust).
The machinery to port exists in the same file: `_collect_imports`
(L1255, the Python path) builds import-map edges, and `_docstring`
(L520) has the per-language branch table Go/Rust currently treat as
empty (the graceful honest-None case the tests pin).

**Plan.**
1. Go: `import_spec_list` → imported package names; file-hub
   resolution via same-directory + `go.mod` module prefix
   approximation (a package maps to its directory's files — document
   the rule); docstrings from block comments immediately preceding
   declarations (doc-comment convention, `//` runs).
2. Rust: `use_declaration` paths → crate-local module resolution
   (`mod` tree within the indexed root, external crates documented as
   out-of-v1 scope — `::` prefixed externals skipped with a count,
   not silently); docstrings from `///` runs.
3. Flip the two documented-gap sentences (extract.py header,
   landscape §7) in the same PR; `tests/test_extract_go_rust.py`
   grows import-edge + docstring assertions (exact sets, both
   languages); D1 no-silent-skip guard stays green.

**Impact / ripples elsewhere.** Call-site resolution may improve as
a side effect (import-resolved candidates feed `resolve_call_edges`'
EXTRACTED path) — measure before/after on the pinned urllib3 corpus
and note it in the PR, don't bury it; benchmark docs only move if
the numbers actually moved.

**Verify.** Import-edge + docstring ground truth per language;
resolution-stats before/after on the pinned corpus published in the
PR description; counts sweep unaffected (no tool-count change).

---

### P3e — graph richness & MCP surface

### [ ] R35 · Non-code artifacts as nodes (M)

**State today (verified).** Extraction is strictly
`_LANG_LOADERS`-keyed by extension (L72) — a Dockerfile, Makefile,
compose yaml, GitHub workflow, or pyproject.toml produces nothing
today (codebase-memory-mcp's "indexes Dockerfiles/K8s as graph
nodes" [vendor claim] is exactly this gap). The relation vocabulary
already has the slot: `DESCRIBES`/`described_by` exist in
`INVERSE_RELATIONS` (`data_model.py`), and `export_html` already
filters TESTS edges by relation name (L74) — the render side is
ready.

**Plan.**
1. An artifact loader pass in `extract.py` (no tree-sitter): known
   filenames (`Dockerfile*`, `Makefile`, `docker-compose*.yml`,
   `.github/workflows/*.yml`, `pyproject.toml`, `go.mod`) → FILE-kind
   nodes with `kind: "FILE"` + `file_type` set (reuse the field).
2. Best-effort DESCRIBES edges, rules stated and test-pinned:
   workflow yaml → paths it touches (checkout/run references —
   coarse, labeled `heuristic` confidence), Makefile → files its
   targets reference, Dockerfile → COPY/ADD sources, pyproject →
   the package's own modules. Anything unresolvable is skipped with
   a counted report in the index payload (never silent).
3. Participation: `affected_by`/R24's impact include them (they're
   FILE nodes — the closure already works), `export_html` files view
   renders them; `freshness_report` counts them.

**Impact / ripples elsewhere.** No new tools (Count Contract
unaffected — extraction-only); index payload gains an
`artifact_files` count; conformance re-run (graceful bucket may
shift if sandbox lacks artifacts — pin whatever it shows).

**Verify.** Fixture with a Dockerfile + workflow yaml asserts node
kinds + DESCRIBES edge sets exactly; index payload carries the
count; export renders them; no silent-skip regression.

### [ ] R36 · Freshness contract (M)

**State today (verified).** `start_watch` (L4157) launches a real
watchdog `Observer` (debounce param, no-op without the watchdog
package — honest), `stop_watch` (L4204), `cie watch` CLI (L1568)
prints the hint, `sync_ast_delta` (L1572) re-parses one file into
the graph, and `freshness_report` (L1348, `stale_after_days=7.0`)
is the only staleness surface. Missing: the loop that applies
deltas on events, and any envelope-level stamp — a read served
after a file edit is indistinguishable from a fresh one.

**Plan.**
1. Wire the existing pieces: the Observer's event handler calls
   `sync_ast_delta(path)` on debounce (same thread discipline the
   task repo's `_ThreadSafeSQLite` already establishes — the
   embedded repo's flush-per-call is the write path); failures land
   in a bounded retry + an honest `watch_errors` counter surfaced
   by `freshness_report`.
2. Stamps: every ToolService read envelope gains `as_of` (the
   index's last-write timestamp — persist one on load/reindex) and
   `stale: bool` (any indexed file's mtime > as_of). Both are
   additive envelope fields (R7's proven pattern).
3. `cie watch` becomes the one-command auto-sync story: print the
   applied-delta log lines, not just the hint.

**Impact / ripples elsewhere.** Envelope tests that assert exact
shapes need the additive fields accounted (grep
`assert set(env` patterns first); `tests/test_cli.py` watch tests
extend; CodeGraph's auto-sync is the [vendor claim] being matched —
landscape phrasing updates when live.

**Verify.** Touch-edit-query loop test with a real Observer (skip
with reason on CI-fs flakiness — document the skip), asserting:
post-debounce query reflects the edit AND `stale` flips true
pre-debounce / false post-apply; unit tests for the stamp math;
`freshness_report` agrees with the stamps.

### [ ] R37 · MCP resources + prompts (M)

**State today (verified).** `cie-mcp` serves tools only — verified:
no `resources`/`prompts` handling in `mcp_server.py`; the server is
the SDK's FastMCP/MCPServer compat layer (shim L23–93, `build_mcp_
server` L96, registration via `add_tool` loop, policy filtering
server-side). GitNexus ships "16 MCP tools + resources + skills"
[scan/vendor claim] — the parity gap is named by the delta.

**Plan.**
1. Resources (read-only, policy-gated like tools):
   `cie://project/stats`, `cie://chains/summary`,
   `cie://export/html` (the R8 artifact, generated on demand),
   `cie://schema` — implemented against the SDK's resource surface
   on **both** 1.x/2.x paths the compat shim already handles (the
   version split is the real work here; the shim's own comment
   block is the map).
2. Prompts: `impact-report` (R24's envelope rendered),
   `traceability-audit`, `onboarding-summary` (what `cie init`
   writes, server-served) — argument schemas per the SDK's prompt
   surface.
3. Policy: resources/prompts respect `permits()` — a non-permitted
   resource refuses server-side with the same envelope discipline
   tools have (test per surface, mirroring the R11 harness).

**Impact / ripples elsewhere.** `tool-test-lab/dogfood_mcp.py` +
`dogfood_mcp_http.py` gain resources/prompts listings; R18's
listing drafts gain the richer surface mention; no tool-count
change (Count Contract untouched — resources aren't tools, say so
in the PR).

**Verify.** Both harnesses list resources+prompts over stdio AND
streamable-http; the policy refusal test pins non-permitted
resource access server-side; 1.x and 2.x SDK both exercised in CI.

### [ ] R38 · `cie serve-ui` — local web viewer (L)

**State today (verified).** The static half shipped (R8:
`cie/export_html.py`, `file://`, zero external refs, screenshot
scripts); its own boundary note anticipates this split ("the safe
slice of a viewer"). The HTTP machinery exists in-repo for the
tool surface (`cie/routes.py`, optional `[http]` extra) but serves
tools, not a UI; no browser UI exists.

**Plan.**
1. `cie serve-ui [PATH] --port` — **localhost bind only, read-only**:
   stdlib `http.server` (or the existing http-extra stack — decide:
   stdlib, to keep it dependency-free like export-html) serving (a)
   the graph JSON (embedded repo reads), (b) a single-page UI
   (inline CSS/JS, R8's template discipline), (c) search + chain +
   callers views. No write endpoints AT ALL — enforced by
   construction (only GET handlers), stated in the threat model.
2. `docs/security.md` gains the serve-ui section: loopback-only,
   single-user trust, read-only, what it deliberately is NOT
   (multi-user, authenticated, remote).
3. Screenshot via the record-script pattern; README links it beside
   the static export (interactive vs frozen — complementary).

**Impact / ripples elsewhere.** Landscape's UI-gap phrasing
(Graphify's clickable graph.html, CodeGraph's platform adjacency)
gains our row; export-html stays the artifact for sharing, serve-ui
for browsing — document that split in both docstrings.

**Verify.** Headless-Chrome harness click-through (the R8 capture
pattern) asserting: renders, searches, no non-GET route accepted
(405 test), loopback bind asserted in a socket test; screenshot
committed via the script.

### [ ] R39 · Multi-project server (M)

**State today (verified).** `cie-mcp` binds one project per process
(`--project` arg, mcp_server.py L134; `factory.build_tool_service_
embedded` L168 constructs one service/graph); the `describe` tool
reflects the one bound graph. Nothing today serves two indexed
projects from one process — a user running multiple repos spins
multiple servers (which works, and stays the default).

**Plan.**
1. `--project` repeatable + `--projects-file` (TOML list of
   `{path, project}` entries): construct one ToolService per
   project (per-DB isolation preserved — this is routing, not
   sharing), route each tool call by its optional `project:` kwarg
   (default: first), server-side (never trust client-side
   filtering).
2. `describe` gains the per-project summary; `describe(project=)`
   detail per graph. MCP: one toolset where tools accept the
   `project` arg (schema-level), not N toolsets — simpler and
   client-friendlier; document the choice.
3. Prerequisite seam for R28 (workspace analytics build ON this
   routing, they don't replace it).

**Impact / ripples elsewhere.** `cie init` unchanged (single-project
default stays); R37's resources gain `project` args; conformance
runs single-project as today (no surface change — arg-only, Count
Contract untouched).

**Verify.** Two-project stdio handshake (the R15 verification
pattern): tools/list identical shape, `callers` with and without
`project=` returns correctly-project-scoped rows, and a
collision test — same symbol name in both projects never leaks
across (id-qualified by construction; assert it anyway).

---

### P3f — credibility & productization

### [ ] R40 · Structural benchmark corpus #4 (M)

**State today (verified).** The R9 harness (`scripts/benchmark.sh`
+ `benchmark_tasks.py`) has produced three datasets (forge 08-28,
requests, urllib3); the landscape's dataset-count sentence says two
small + one well-known — CodeGraph's published range is 7 repos
[vendor claim]. C parses (R12/R13) but no C corpus has been run
through it.

**Plan.**
1. Use R31's chosen C corpus (one index serves both items — pin the
   SAME commit in both docs); run the harness end-to-end; publish
   `docs/benchmarks-<repo>.md` in the exact two-column honesty
   format including the honest-loss section (pre-commit to
   publishing whatever resolves badly — the R9 rule).
2. Landscape + README dataset sentences update with dated re-runs;
   CHANGELOG entry.

**Verify.** Fresh-clone reproduction from the script alone (GFI-3's
bar — this run doubles as the first live check of that issue's
premise); doc tables are script output with run date + commit.

### [ ] R41 · Token accounting, pinned (S)

**State today (verified).** All token figures are chars//4
(`graphrag.py` L54, AI-06's documented approximation); every
benchmark doc's "token" column inherits it; the vendor "120× fewer
tokens" claim (codebase-memory-mcp) has no apples-to-apples
counterpart on our side.

**Plan.**
1. After R31's tokenizer extra lands: regenerate every historical
   benchmark table with both chars and pinned-token columns
   (script-driven, dated re-run notes where numbers move).
2. The explicit comparison paragraph in the benchmarks doc:
   ours-measured vs vendor-claim, labeled, with the standing
   no-implied-equivalence rule quoted.
3. A grep-clean pass: no prose token number without a table row
   behind it (the R3 sweep discipline, applied to tokens).

**Verify.** Docs regenerate from scripts with both metrics;
`grep -rn "token"` review shows every claim table-backed; R20's
audit table row 14 gains the pinned source.

### [ ] R42 · Scale milestone: ≥100k-LOC corpus (M)

**State today (verified).** Largest measured corpus: urllib3
(~12k LOC, 1,207 nodes, 5.9s warm-min index). SocratiCode publishes
"2.45M-LOC benchmark / 61% less context" [vendor claims]. One
in-repo risk is already known by design:
`EmbeddedRepository.__getattribute__` re-persists the WHOLE graph
after every public call (`_flush` → `save_graph` DELETE+INSERT,
embedded_repository.py L173–186) — fine at 1–10k nodes, a real
cost at 50k+. The milestone should measure it, not discover it.

**Plan.**
1. Corpus: django (pinned commit) or a cpython `Lib/` subset — pick
   on determinism (vendored files, stable tree), state the pick.
2. Measure and publish: index time + peak memory (tracemalloc or
   /usr/bin/time -v), per-tool latency for the canonical task
   shapes, and the flush overhead breakdown (calls × graph size) —
   with the honest note that embedded is the zero-config path and
   Neo4j is the scale path (measure both if a Neo4j env is
   available; otherwise state embedded-only).
3. Follow-ups surface as their own items (e.g. flush-to-delta
   persistence) — not surprises folded into this PR; the milestone
   doc targets, not fixes.

**Impact / ripples elsewhere.** Landscape's scale-phrasing gains
our measured number; R20's post gains the scale row; conformance
runs unchanged (fixture-scoped).

**Verify.** Dated results doc, script-reproducible (a `--corpus
django` mode for benchmark.sh or a sibling script); numbers
regenerate within stated variance on a second run.

### [ ] R43 · `cie doctor` + init breadth (S)

**State today (verified).** No doctor command exists (cli.py's
groups: index/query/tasks/hierarchy/sync/serve/init/export…).
`cie init` (cie/init.py) detects claude-code (`~/.claude.json` or
project `.mcp.json`, L69–70) and cursor (`~/.cursor`, L71–72),
prints Codex TOML without editing it; no Cline/Continue/Windsurf/
VSCode-agent detection. A real MCP self-handshake client exists
(`tool-test-lab/dogfood_mcp_stdio_list.py` — reads the registered
entry, spawns it, lists tools).

**Plan.**
1. `cie doctor [PATH]`: index freshness (mtimes vs index stamp —
   the R36 seam, or freshness_report's logic if R36 hasn't landed),
   db sanity (schema/version probe), orphan tasks/tests summary,
   env echo (NEO4J_*/CIE_* presence, redacted), backend resolution
   (which seam would fire), and — the trust piece — an MCP
   self-handshake of the registered client entry (spawn + tools/
   list + count), reporting each finding with a fix hint. Read-only
   by construction.
2. Init breadth: Cline (`~/.cline/`? — verify actual config path
   at implementation, don't guess in the plan), Continue, Windsurf,
   VSCode-agent config presence, R15's guarded-merge discipline
   (byte-preserve existing entries, refuse invalid JSON).
3. Both get CLI tests at `tests/test_cli.py`'s bar (CliRunner,
   fixtures, no monkeypatched openers).

**Impact / ripples elsewhere.** README troubleshooting section
links doctor; R19's issue template could ask for `cie doctor`
output (add to CONTRIBUTING's bug-report section); init's README
section grows the client table.

**Verify.** Doctor on a healthy fixture = all-green output; on a
broken fixture (stale index, orphaned task, bad env) = each finding
asserted; one new client config merged idempotently (re-run =
no-op).

### [ ] R44 · Gate packs (S/M)

**State today (verified).** Every governance checker is an
individual conformance-verified tool: `traceability_coverage`,
`invariant_violations`, `check_invariant`, `clone_detect_run`,
`drift_detect_run`, `coverage_gaps`, `tech_debt_report`,
`test_skeletons_run` — but composing them for a CI decision (which
checkers, what thresholds, what exit code) is entirely on the
caller. No gate command exists.

**Plan.**
1. `cie gate --profile pr|nightly|refactor [--config FILE]`:
   profiles map to checker sets + thresholds (pr: traceability
   coverage ≥ X, no invariant violations, coverage gaps ≤ Y;
   nightly: everything + clones/drift/tech-debt; refactor: clones +
   drift + semantic-diff summary). Config file = TOML override of
   the built-in profiles (R27's loader discipline).
2. Output: JSON (machine) + hint lines (human), exit codes 0/1
   (2 = profile/config error — fail fast, R27's rule); the `pr`
   profile calls R24's impact when a diff is present.
3. CI recipe in README (the one-yaml usage) + a fixture-repo
   example wired into this repo's own CI as a proof.

**Impact / ripples elsewhere.** No tool-surface change (CLI
composition — Count Contract untouched, note it in the PR);
landscape's governance paragraph gains "CI-consumable as one
command"; pairs with R22's matrix and R17's gate pattern.

**Verify.** The R17 proof shape: a fixture repo where an
intentionally-broken commit turns the gate red and restore turns
it green, asserted in CI; profile thresholds pinned by tests.

### [ ] R45 · Last-503s: reference decompose plugin (S)

**State today (verified).** `decompose_page` is the one
`HOST_PLUGIN_MISSING` tool: `cie/decompose.py` exposes the plugin
contract — `register_html_walker_factory` (L83) /
`register_interactive_element_detector` (L92) — and raises
`DetectorUnavailable` (L117) with the honest reason when nothing
is registered; `_guard` maps it to the pinned reason slug. Once
R29 lands, this is the ONLY forced-503 left; R5's registry test
(`EXPECTED_REASONS`) currently pins the 5-tool bucket and would
need its end-state update in the same commit as any change.

**Plan.**
1. Reference detector in `cie/decompose_reference.py` (or the
   examples dir — decide: in-package, so the example is testable
   without install gymnastics): `html.parser`-based interactive-
   element extraction (buttons/links/inputs/forms with ids +
   text), registered explicitly via the existing contract —
   OPT-IN import (`from cie.decompose_reference import install`),
   never auto-registered (the plugin boundary stays meaningful).
2. `docs/plugin.md`: the extension contract (both registration
   seams, the reason-slug discipline, the R5/R10 history of why
   gates exist), with the reference detector as the worked example.
3. Registry end-state: with R29 + this, `EXPECTED_REASONS`
   becomes env/plugin-conditional — the registry test pins the
   END STATE (0 forced-503s with everything opted in; the env-less
   default stays honestly at today's bucket) — both conditions
   asserted, same file.

**Impact / ripples elsewhere.** README's "~5 tools require the
host environment" footnote ends at the R29/R45 pair (flip to "all
optional backends are opt-in, none forced"); landscape +
CHANGELOG; `docs/adding-a-language.md`'s plugin-adjacent docs link
`plugin.md`.

**Verify.** Fixture screen (HTML) → detector returns the
known-by-inspection element set; `decompose_page` works with the
plugin registered, 503s with the same reason slug without; the
registry test asserts both conditions; conformance artifacts for
both states committed.

---

### P3 execution order (recommended, with the gates)

R21 (PyPI upload — needs owner creds) → R22 → R23 (mechanics spine)
→ **R24** (impact: the W1 answer; highest-leverage) → R26 (overlay
feeds R24's ranking; either order acceptable, note in PR) → R25
(review pack composes R24) → **R29** (chat fallback: un-503s four
tools + completes the semantic story) → R45 (ends the forced-503
story) → R30 → R31 → R34 → R32 → R33 (languages) → R35 → R36 → R37
→ R39 → R38 → R27 → R40 → R41 → R42 → R43 → R44. **R28 stays
gated on its watch-list trigger (a real multi-repo user).** The
re-sequencing rule overrides everything: a `⚠ DIFFERENTIATOR
IMPACT` finding from the next delta scan pre-empts this tier.

---
## Session log

- **2026-08-30 (planning pass):** read roadmap + goal + CONTRIBUTING;
  walked `cie/routes.py`, `cie/mcp_server.py`, `cie/tool_policy.py`,
  `cie/tools/__init__.py`, `cie/tools/heuristic.py`, `cie/cli.py`,
  `cie/factory.py`, `cie/embedded_repository.py`,
  `cie/embedded_task_repository.py`, `cie/hierarchy.py`,
  `cie/extract.py`, `cie/callgraph.py`, `tests/*` (invariants,
  ground-truth, smoke), `tool-test-lab/*` (harness + results),
  `.github/workflows/ci.yml`, README/CHANGELOG/docs counts. Findings
  folded into per-item "State today" sections. Notable: R11's transport
  choice partially exists in the CLI parser already; R14's referenced
  in-memory fake is missing on disk; surface_conformance.py has a
  machine-local PYTHONPATH hardcode (blocks R17 until fixed);
  `ToolService` measures exactly 126 public methods on this commit.
- **2026-08-30 (implementation pass 1)** — R4 done: fold executed
  (tombstone + goal.md Provenance section + 11 reference rewrites),
  verified clean. Committed.
- **2026-08-30 (implementation pass 2)** — R2 done: CLI↔SQLite parity.
  Also fixed en route: the venv's cie was a stale non-editable
  site-packages copy (the conformance-README trap) → now editable;
  suite 171 → 184; quickstart verified against psf/requests in a clean
  venv (858 nodes/1,822 edges; callers("close") = 3, matching
  docs/benchmarks-requests.md). Committed.
- **2026-08-30 (implementation pass 3)** — R1 done minus `push_hierarchy`
  (blocked on R14's SQLite hierarchy store by design, alias stays).
  Found en route (honest note, not fixed here — pre-existing semantics
  preserved by the promotion): the "push is idempotent per task name"
  hint is only true idempotence when the batch re-pushes the same task
  id; a fresh-id re-push with the same name creates a second row (both
  backends; plan_push rejects duplicate names within one batch only).
  Documented by test. Suite 186 → 205. Committed.
- **2026-08-30 (implementation pass 4)** — R5 done: 18 → 5 unavailable,
  all with reason slugs; 13 tools now run purely standalone; bucket
  scan caught and fixed `failing_context("")` IsADirectoryError;
  conformance 100/23/5/4, 0 crashes; suite 205 → 212. Committed.
- **2026-08-30 (implementation pass 5)** — R14 done: SQLite hierarchy
  store + the last three HTTP-only tools promoted; HTTP_WRITE_ALIASES
  empty; surface 132 → 135 (conformance 100/26/5/4, 0 crashes); suite
  213 → 235. The R3-era "hierarchy still Neo4j-only" caveat flipped in
  README; hierarchy.py's nonexistent-fake docstring pointer fixed.
  Committed.
- **2026-08-30 (implementation pass 6)** — R7 done: provenance labels +
  call-resolution tallies visible in callers/callees output; live psf/
  requests check (resolution 19/16/3, name-keyed reconciled against the
  doc's 6-site framing); suite 235 → 239. Committed.
- **2026-08-30 (implementation pass 7)** — R11 done: streamable-http
  end to end (wiring + harness + docs); suite 239 → 241. Committed.
- **2026-08-30 (implementation pass 8)** — R8 done: export-html feature
  (CLI + module + tests), psf/requests artifact + screenshots recorded
  via the script; suite 241 → 246. Committed.
- **2026-08-30 (implementation pass 9)** — R9 done: harness + third
  dataset (urllib3), README benchmark paragraph links the third repo;
  suite 246 still green (scripts-only + doc). Committed.
- **2026-08-30 (implementation pass 10)** — R12 done: C language #7
  (declarator unwrap + mirror-bug pin + guard suite); suite 246 → 254.
  Committed.
- **2026-08-30 (implementation pass 11)** — R13 done: C++ (#8) + C# (#9);
  two live-found fixes (`.h` mis-tree under C grammar for C++ —
  re-scoped with docs; `cs`-vs-`csharp` language-key mismatch); suite
  254 → 264. Committed.
- **2026-08-30 (implementation pass 12)** — R17 done (CI conformance
  gate + approved_surface.json + fixture; one collection leak fixed);
  R16 done (docs/security.md + CIE_RUN_WRAPPER seam + spawn-guard test);
  suite 264 → 266. Committed in two commits.
- **2026-08-30 (implementation pass 13)** — R15 done: cie init
  (detection + guarded merges + managed context blocks + readonly-by-
  default), verified via a real stdio handshake on the registered entry;
  suite 266 → 279. Committed.
- **2026-08-31 (implementation pass 14)** — R19 applied end-to-end:
  repo description + 10 topics live (gh repo edit), starter issues #17/
  #18/#19 created against CONTRIBUTING's safe-entry list; draft doc
  committed with apply-stamps. En-route finding recorded under R6:
  pypi.org/project/cie is cluster311/cie10 — the name-availability
  assumption in R6's step 4 is false; needs a package-name decision.
  README contributing link verified already in (5b527c4).
- **2026-08-31 (implementation pass 15)** — R6 executed end-to-end
  (minus PyPI upload, blocked on a package-name decision): version
  house, artifacts built + twine-passed, clean-venv rehearsal from the
  wheel on the third-party repo (CLI + real MCP client, all matching
  published numbers), tag `v0.1.0` + GH release with caveats kept, C1
  hand-closed at tag `v0.1.0` on `a22b4bf`. Committed in two commits
  (release cut; goal/todo close-out). PyPI finding: pypi `cie` =
  cluster311/cie10; README quickstart switched to GitHub installs with
  a dated note until the distribution name is decided.
- **2026-08-31 (implementation pass 16)** — R10 done end-to-end: (step
  1) first-party OpenAI-compatible embeddings fallback (stdlib, DSN+key
  gated, no-accidental-network rule pinned) + embedded-load enrichment +
  graphrag pure layers decoupled (qa envelope unchanged, re-pinned);
  suite 279 → 292; conformance 135 tools / 101-25-5-4 / 0 crashes, fresh
  artifact committed. (step 2) scripts/benchmark_semantic.py + live runs
  on both R9 corpora via NIM nv-embed-1b fallback: recall@8 = 1.0 on
  16/16 hand-labeled questions, MRR 0.75–0.85 per retriever per corpus
  (neither dominates — published both ways), index overhead ~free;
  docs/competitor-benchmarks.md dated section with raw-JSON artifacts +
  vendor-config table (claude-context/grepai labeled not-run-here).
  Committed in two commits.
- **2026-08-31 (implementation pass 17)** — R18 step 1 + R20 steps 1–2:
  directory-listings drafts (docs/directory-listings-drafts.md, nothing
  submitted, gate now satisfied → only the go-ahead remains) and the
  launch-post draft (docs/launch-post-draft.md, 14-row claim audit,
  every row footnoted; publish gates 1–3/7 satisfied, 4–6 open). Boxes
  stay open until the external actions actually happen, per their own
  gating. Committed separately.
- **2026-08-31 (implementation pass 18)** — PyPI distribution renamed to
  **`cie-mcp`** (user's decision; pypi `cie` was confirmed unrelated):
  pyproject name + version 0.1.1, README quickstart/install switched to
  `pip install "cie-mcp[mcp]"` with the dated note, live install strings
  swept in benchmarks/reproducing docs + the R18/R20 drafts,
  competitive-landscape + goal.md C1 follow-up + repo-trust-signals
  notes updated; suite green; artifacts rebuilt and re-rehearsed under
  the new dist name; tag `v0.1.1` + GH release cut. The upload itself
  runs with the maintainer's PyPI token or trusted-publisher workflow
  (no credentials available in this environment — handoff noted in the
  release notes). Next update appends here.
- **2026-08-31 (implementation pass 19)** — competitive-scan docs made
  maintainer-local per the owner's request: `docs/competitive-
  landscape.md` + `docs/competitor-benchmarks.md` untracked
  (`git rm --cached`; gitignore + working-tree copies retained), .gitignore
  entries added, live click-through references in README + docs/
  benchmarks.md R18/R20 drafts rewritten to point at the public
  benchmark docs instead (frozen dated records keep their path
  mentions per convention). NOTE: the files remain visible in git
  HISTORY (old commits); a purge would need a history rewrite + force-
  push — not done.
- **2026-08-31 (planning pass 2)** — P3 drafted from the two source docs:
  delta WATCH W1–W4 formally PROMOTED (R24/R25, R27, R26, R28 — R28
  keeping its real-user gate per the re-sequencing rule), landscape
  honest-gaps mapped to R21/R22/R32–R34/R40–R43, semantic-completion
  thread to R29–R31/R45, surface-parity thread to R35–R39, governance
  productization to R44. 25 items, each with basis · size · verify;
  execution order in todo.md P3. Facts verified at planning time:
  ubuntu-only CI matrix; no MCP resources/prompts surface;
  sync_load_commit/sync_ast_delta/record_test_result/TestExecution all
  exist (impact/overlay are composites, not new graph machinery); no
  tokenizer dep; watchdog Observer present in start_watch.
- **2026-08-31 (implementation pass 20)** — native client compatibility
  fixed + live-verified: `cie init` no longer registers a bare
  `"cie-mcp"` command (unresolvable by GUI-launched clients whose
  PATH lacks the venv bin — the real-world spawn failure); entries are
  now spawn-robust (absolute script path, else the running interpreter
  via `-m cie.mcp_server`) in `.mcp.json`, Cursor's config, and the
  Codex TOML snippet alike; the managed context block shows the same
  command. Verified NATIVELY with the real Claude Code CLI (2.1.251):
  `cie init` → `claude mcp list` ✔ Connected → a one-shot
  `claude -p` agent call of `search_symbol` through the server returned
  the correct indexed file, entry spawned exactly as registered;
  test-side PATH compensation removed (the entry is spawned as
  written). Cursor/Codex not installed here: format-verified against
  their documented config shapes (Cursor shares Claude Code's JSON
  shape; Codex `[mcp_servers.*]` TOML) — noted as such, not claimed as
  live. Suite 292 → 294 (init tests: 15 incl. two new resolution tests).
- **2026-08-31 (implementation pass 21)** — one-click install answered
  with a verified release, not a claim. Gap found: README had install
  strings but no one-click story, and the client deep-link docs were
  unreachable for curl — and cie doesn't fit browser one-click anyway
  (local stdio + per-project path + a policy the operator chooses —
  exactly what `cie init` computes). Cut **v0.1.2** (111e10e, tag + GH
  release live) so the one-click installs a ref that CONTAINS the
  spawn-robust fix: README top now carries the verified one-click
  (`uv tool install "cie-mcp[mcp] @ …@v0.1.2"` once, `cie init .`
  per project) + the `claude mcp add` one-liner + a dated note stating
  exactly what was live-verified vs format-verified; CHANGELOG
  [0.1.2] carries the fix entry, the one-click entry, and R10's entry
  moved VERBATIM from [Unreleased] with an attribution note (it
  shipped in the v0.1.1 artifact — b6393de/279ee53 in tag v0.1.1 —
  but had stayed under [Unreleased] through the rename cut). The
  chain was run against the RELEASED tag exactly as README documents:
  uv tool install @v0.1.2 → `cie`/`cie-mcp` on PATH (0.1.2) → `cie
  init` writes the ABSOLUTE `~/.local/bin/cie-mcp` entry (resolution
  order 1, live) → `claude mcp list` ✔ Connected → one-shot `claude
  -p` `search_symbol` returns the correct file. Artifacts:
  cie_mcp-0.1.2 sdist+wheel, twine PASSED, clean-venv rehearsal
  (85 read tools, real tool call). Suite 294. Next update appends here.
- **2026-08-31 (implementation pass 22)** — multi-project question
  answered by measurement, not opinion: (1) live-verified that
  multiple projects work TODAY with zero new code — `cie-web` +
  `cie-api` registered in one Claude Code config, both `✔ Connected`,
  and one `claude -p` session calling both servers' `search_symbol`
  returned the correct per-project files (client-side tool
  namespacing by server name); (2) recorded in R28's addendum:
  Neo4j mode already namespaces multi-project (`--project`, node
  `project` property + filters), and the micro-gap found — `cie init`
  registers the fixed name `"cie"`, so a user-scope config holds one
  cie entry (candidate: `--name cie-<alias>`); (3) R28's gate kept:
  the owner's question is an interest signal, not a real workspace.
  README one-click section gained the "Multiple projects?" note with
  the dated live verification. No code change this pass. Suite 294
  (ritual). Next update appends here.